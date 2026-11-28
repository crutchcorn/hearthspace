use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Store identity and instance sequence are unrelated to outputs or live surfaces.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceId {
    pub(crate) namespace: Uuid,
    pub(crate) serial: u64,
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}", self.namespace, self.serial)
    }
}

impl std::str::FromStr for InstanceId {
    type Err = super::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (namespace, serial) = value
            .split_once(':')
            .ok_or(super::Error::InvalidField("widget identity"))?;
        let namespace = Uuid::parse_str(namespace)
            .map_err(|_| super::Error::InvalidField("widget identity namespace"))?;
        let serial = serial
            .parse::<u64>()
            .map_err(|_| super::Error::InvalidField("widget identity sequence"))?;
        if namespace.is_nil() || serial == 0 {
            return Err(super::Error::InvalidField("widget identity"));
        }
        let id = Self { namespace, serial };
        if id.to_string() != value {
            return Err(super::Error::InvalidField("noncanonical widget identity"));
        }
        Ok(id)
    }
}
