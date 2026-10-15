//! AccessKit implements GetRole but omits GetRoleName. Enrich only unknown
//! roles from Waydriver's snapshot, retaining its escaping, state, and identity.
use std::collections::HashSet;

use quick_xml::{
    Reader, Writer,
    events::{BytesEnd, BytesStart, Event},
};
use waydriver::{Error, Result};

pub(super) async fn resolve_numeric_roles(
    connection: &zbus::Connection,
    xml: &str,
) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Vec::new());
    let mut tags = Vec::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|error| Error::atspi_with("read accessibility XML", error))?;
        let event = match event {
            Event::Start(start) => {
                let start = resolve_start(connection, start).await?;
                tags.push(String::from_utf8_lossy(start.name().as_ref()).into_owned());
                Event::Start(start)
            }
            Event::Empty(start) => Event::Empty(resolve_start(connection, start).await?),
            Event::End(_) => Event::End(BytesEnd::new(
                tags.pop()
                    .ok_or_else(|| Error::atspi("unbalanced accessibility XML"))?,
            )),
            Event::Eof => break,
            event => event.into_owned(),
        };
        writer
            .write_event(event)
            .map_err(|error| Error::atspi_with("write accessibility XML", error))?;
    }
    String::from_utf8(writer.into_inner())
        .map_err(|error| Error::atspi_with("accessibility XML encoding", error))
}

async fn resolve_start(
    connection: &zbus::Connection,
    start: BytesStart<'_>,
) -> Result<BytesStart<'static>> {
    let start = deduplicate_attributes(start)?;
    if start.name().as_ref() != b"Unknown" {
        return Ok(start.into_owned());
    }
    let reference = start
        .try_get_attribute("_ref")
        .map_err(|error| Error::atspi_with("accessibility reference", error))?
        .map(|attribute| {
            attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|value| value.into_owned())
        })
        .transpose()
        .map_err(|error| Error::atspi_with("accessibility reference encoding", error))?;
    let Some((bus, path)) = reference
        .as_deref()
        .and_then(|reference| reference.split_once('|'))
    else {
        return Ok(start.into_owned());
    };
    let proxy = zbus::Proxy::new(connection, bus, path, "org.a11y.atspi.Accessible").await;
    let Ok(proxy) = proxy else {
        return Ok(start.into_owned());
    };
    let role: zbus::Result<u32> = proxy.call("GetRole", &()).await;
    let Some(role) = role
        .ok()
        .and_then(|value| atspi::Role::try_from(value).ok())
    else {
        return Ok(start.into_owned());
    };
    rename_role(start, role.name())
}

fn deduplicate_attributes(start: BytesStart<'_>) -> Result<BytesStart<'static>> {
    // Waydriver emits canonical properties, then toolkit attributes, then the
    // canonical _ref. Firefox supplies description in both sets, producing XML
    // that XPath rejects. Keep canonical fields and the final stable identity.
    let mut result = start.to_owned();
    result.clear_attributes();
    let mut seen = HashSet::new();
    let mut reference = None;
    for attribute in start.attributes().with_checks(false) {
        let attribute =
            attribute.map_err(|error| Error::atspi_with("accessibility attributes", error))?;
        if attribute.key.as_ref() == b"_ref" {
            reference = Some(attribute);
        } else if seen.insert(attribute.key.as_ref().to_vec()) {
            result.push_attribute(attribute);
        }
    }
    if let Some(reference) = reference {
        result.push_attribute(reference);
    }
    Ok(result)
}

fn rename_role(start: BytesStart<'_>, role: &str) -> Result<BytesStart<'static>> {
    let (tag, _) = waydriver::atspi::element_role_fields(role);
    let mut result = BytesStart::new(tag);
    for attribute in start.attributes() {
        let attribute =
            attribute.map_err(|error| Error::atspi_with("accessibility attributes", error))?;
        if attribute.key.as_ref() != b"role" {
            result.push_attribute(attribute);
        }
    }
    result.push_attribute(("role", role));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_role_keeps_escaped_names_and_stable_identity() {
        let mut reader = Reader::from_str(
            r#"<Unknown role="unknown" name="Save &amp; close" _ref=":1.2|/button" focusable="true"/>"#,
        );
        let Event::Empty(start) = reader.read_event().unwrap() else {
            panic!("node");
        };
        let mut writer = Writer::new(Vec::new());
        writer
            .write_event(Event::Empty(
                rename_role(start, atspi::Role::Button.name()).unwrap(),
            ))
            .unwrap();
        let xml = String::from_utf8(writer.into_inner()).unwrap();
        let matches =
            waydriver::atspi::evaluate_xpath_detailed(&xml, "//Button[@name='Save & close']")
                .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].ref_.1, "/button");
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("focusable=\"true\""));
    }

    fn normalized(xml: &str) -> Result<String> {
        let mut reader = Reader::from_str(xml);
        let mut writer = Writer::new(Vec::new());
        loop {
            let event = match reader.read_event().unwrap() {
                Event::Start(start) => Event::Start(deduplicate_attributes(start)?),
                Event::Empty(start) => Event::Empty(deduplicate_attributes(start)?),
                Event::Eof => break,
                event => event.into_owned(),
            };
            writer.write_event(event).unwrap();
        }
        Ok(String::from_utf8(writer.into_inner()).unwrap())
    }

    #[test]
    fn firefox_duplicate_description_does_not_break_unrelated_semantic_targets() {
        let xml = normalized(
            r#"<Application _ref=":1.3|/app">
          <Link role="link" name="Weather" description="AccuWeather® ∙ Sponsored"
            visible="true" description="AccuWeather® ∙ Sponsored" _ref=":1.3|/weather"/>
          <Entry role="entry" name="Browser text" _ref=":1.3|/entry"/>
        </Application>"#,
        )
        .unwrap();
        let matches =
            waydriver::atspi::evaluate_xpath_detailed(&xml, "//Entry[@name='Browser text']")
                .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].ref_.1, "/entry");
        let weather = waydriver::atspi::evaluate_xpath_detailed(&xml, "//Link").unwrap();
        assert_eq!(
            weather[0].description.as_deref(),
            Some("AccuWeather® ∙ Sponsored")
        );
    }

    #[test]
    fn toolkit_collisions_cannot_replace_canonical_fields_or_object_identity() {
        let xml = normalized(
            r#"<Button role="button" name="Save &amp; close" focused="true"
          role="toolkit-role" name="toolkit-name" focused="false" description="First"
          description="Second" _ref=":1.9|/toolkit" _ref=":1.2|/button"/>"#,
        )
        .unwrap();
        let matches = waydriver::atspi::evaluate_xpath_detailed(
            &xml,
            "//Button[@name='Save & close' and @role='button' and @focused='true']",
        )
        .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].ref_, (":1.2".into(), "/button".into()));
        assert_eq!(matches[0].description.as_deref(), Some("First"));
        assert!(xml.contains("&amp;"));
    }

    #[test]
    fn deduplication_still_rejects_malformed_attributes() {
        assert!(normalized(r#"<Button name="Save" malformed _ref=":1.2|/button"/>"#).is_err());
    }
}
