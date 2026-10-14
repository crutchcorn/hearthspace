//! Semantic targeting on an explicitly selected Hearthspace session bus.
//!
//! Read `dbus_session_bus_address` from the native launcher's session.json.
//! These helpers never consult the calling agent's host-session environment.
//! Activation reports AT-SPI acceptance; verify the resulting UI with a fresh
//! capture and an application-specific assertion afterwards.

use std::time::Duration;

use waydriver::{Error, Result, atspi};

pub use waydriver::atspi::ElementInfo;

pub struct SessionAccessibility {
    session_bus_address: String,
}

impl SessionAccessibility {
    pub fn new(session_bus_address: impl Into<String>) -> Result<Self> {
        let session_bus_address = session_bus_address.into();
        if session_bus_address.trim().is_empty() {
            return Err(Error::atspi(
                "an explicit Hearthspace session bus address is required",
            ));
        }
        Ok(Self {
            session_bus_address,
        })
    }

    /// Enable accessibility bridges (including AccessKit) on this session's
    /// private bus. Call once before discovery; the host bus is never consulted.
    pub async fn enable(&self, timeout: Duration) -> Result<()> {
        tokio::time::timeout(timeout, async {
            let connection =
                waydriver::gaction::connect_session_bus(&self.session_bus_address).await?;
            let proxy = zbus::Proxy::new(
                &connection,
                "org.a11y.Bus",
                "/org/a11y/bus",
                "org.a11y.Status",
            )
            .await
            .map_err(|e| Error::atspi_with("accessibility status proxy", e))?;
            proxy
                .set_property("ScreenReaderEnabled", true)
                .await
                .map_err(|e| Error::atspi_with("enable private-session accessibility", e))
        })
        .await
        .map_err(|_| Error::Timeout("AT-SPI enable deadline exceeded".into()))?
    }

    /// Snapshot the registry and its application trees as searchable XML.
    pub async fn snapshot(&self, timeout: Duration) -> Result<String> {
        tokio::time::timeout(timeout, async {
            let connection = atspi::connect_a11y(&self.session_bus_address).await?;
            atspi::snapshot_tree(
                &connection,
                "org.a11y.atspi.Registry",
                "/org/a11y/atspi/accessible/root",
            )
            .await
        })
        .await
        .map_err(|_| Error::Timeout("AT-SPI snapshot deadline exceeded".into()))?
    }

    /// Query by role/name, e.g. `//PushButton[@name='Save']`.
    pub async fn find(&self, xpath: &str, timeout: Duration) -> Result<Vec<ElementInfo>> {
        atspi::evaluate_xpath_detailed(&self.snapshot(timeout).await?, xpath)
    }

    /// Activate exactly one semantic match. Ambiguous selectors never act.
    pub async fn activate(&self, xpath: &str, timeout: Duration) -> Result<()> {
        tokio::time::timeout(timeout, async {
            let connection = atspi::connect_a11y(&self.session_bus_address).await?;
            let snapshot = atspi::snapshot_tree(
                &connection,
                "org.a11y.atspi.Registry",
                "/org/a11y/atspi/accessible/root",
            )
            .await?;
            let element = unique_match(&snapshot, xpath)?;
            atspi::do_action_on(&connection, xpath, &element.ref_.0, &element.ref_.1).await
        })
        .await
        .map_err(|_| Error::Timeout("AT-SPI activation deadline exceeded".into()))?
    }
}

fn unique_match(snapshot: &str, xpath: &str) -> Result<ElementInfo> {
    let mut matches = atspi::evaluate_xpath_detailed(snapshot, xpath)?;
    if matches.len() != 1 {
        return Err(Error::atspi(format!(
            "semantic action requires one match; selector {xpath:?} matched {}",
            matches.len(),
        )));
    }
    Ok(matches.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_action_requires_an_unambiguous_match() {
        let xml = r#"<Application _ref=":1.2|/app">
            <PushButton _ref=":1.2|/save" name="Save"/>
            <PushButton _ref=":1.2|/cancel" name="Cancel"/>
        </Application>"#;
        let save = unique_match(xml, "//PushButton[@name='Save']").unwrap();
        assert_eq!(save.ref_.1, "/save");
        assert!(unique_match(xml, "//PushButton").is_err());
        assert!(unique_match(xml, "//PushButton[@name='Missing']").is_err());
    }

    #[test]
    fn explicit_bus_is_required() {
        assert!(SessionAccessibility::new("").is_err());
        assert!(SessionAccessibility::new("  ").is_err());
        assert!(SessionAccessibility::new("unix:path=/tmp/hearthspace-test-bus").is_ok());
    }
}
