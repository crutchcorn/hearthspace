use super::{Error, WidgetKind};
use serde::{Deserialize, Serialize};

pub const MAX_DATE_FORMAT_BYTES: usize = 128;

/// Configuration owned by one copy. Readings and shared service state are external.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub expanded_details: bool,
    #[serde(default)]
    pub hide_when_unavailable: bool,
    /// Renderer-specific formatting remains external; this core bounds its input.
    pub clock_date_format: Option<String>,
}

impl Preferences {
    pub(crate) fn validate(&self, kind: WidgetKind) -> Result<(), Error> {
        let Some(format) = &self.clock_date_format else {
            return Ok(());
        };
        if kind != WidgetKind::Clock {
            return Err(Error::InvalidField("date format requires a clock"));
        }
        if format.is_empty() || format.len() > MAX_DATE_FORMAT_BYTES {
            return Err(Error::InvalidField("clock date format length"));
        }
        if format.chars().any(char::is_control) {
            return Err(Error::InvalidField("clock date format characters"));
        }
        Ok(())
    }
}
