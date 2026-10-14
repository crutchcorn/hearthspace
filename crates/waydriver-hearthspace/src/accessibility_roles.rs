//! AccessKit implements GetRole but omits GetRoleName. Enrich only unknown
//! roles from Waydriver's snapshot, retaining its escaping, state, and identity.
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
}
