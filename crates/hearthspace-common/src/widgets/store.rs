use super::{Error, InstanceId, Preferences, WidgetKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const MAX_INSTANCES: usize = 1024;
// More than the maximum valid snapshot, including worst-case string escaping.
pub const MAX_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetInstance {
    id: InstanceId,
    kind: WidgetKind,
    preferences: Preferences,
}

impl WidgetInstance {
    pub fn id(&self) -> InstanceId {
        self.id
    }

    pub fn kind(&self) -> WidgetKind {
        self.kind
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }
}

/// A single-writer collection. Placement belongs to an external arrangement owner.
/// Persist its state and this collection together after coordinated changes.
/// Old-backup recovery and concurrent writer arbitration remain external.
#[derive(Debug)]
pub struct WidgetStore {
    namespace: Uuid,
    next_serial: u64,
    instances: BTreeMap<InstanceId, WidgetInstance>,
}

impl Default for WidgetStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetStore {
    /// Creates an empty collection with a fresh identity namespace.
    pub fn new() -> Self {
        Self {
            namespace: Uuid::new_v4(),
            next_serial: 1,
            instances: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, kind: WidgetKind, preferences: Preferences) -> Result<InstanceId, Error> {
        preferences.validate(kind)?;
        if self.instances.len() >= MAX_INSTANCES {
            return Err(Error::TooManyInstances);
        }
        let next_serial = self
            .next_serial
            .checked_add(1)
            .ok_or(Error::IdentityExhausted)?;
        let id = InstanceId {
            namespace: self.namespace,
            serial: self.next_serial,
        };
        let instance = WidgetInstance {
            id,
            kind,
            preferences,
        };
        self.instances.insert(id, instance);
        self.next_serial = next_serial;
        Ok(id)
    }

    pub fn get(&self, id: InstanceId) -> Option<&WidgetInstance> {
        self.instances.get(&id)
    }

    pub fn instances(&self) -> impl Iterator<Item = &WidgetInstance> {
        self.instances.values()
    }

    pub fn set_preferences(
        &mut self,
        id: InstanceId,
        preferences: Preferences,
    ) -> Result<(), Error> {
        let instance = self
            .instances
            .get_mut(&id)
            .ok_or(Error::UnknownInstance(id))?;
        preferences.validate(instance.kind)?;
        instance.preferences = preferences;
        Ok(())
    }

    pub fn remove(&mut self, id: InstanceId) -> Result<WidgetInstance, Error> {
        self.instances.remove(&id).ok_or(Error::UnknownInstance(id))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            schema_version: SCHEMA_VERSION,
            namespace: self.namespace,
            next_serial: self.next_serial,
            instances: self.instances.values().cloned().collect(),
        }
    }

    pub fn to_json(&self) -> Result<String, Error> {
        let snapshot = self.snapshot();
        let json = serde_json::to_string(&snapshot).map_err(json_error)?;
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(Error::InputTooLarge);
        }
        Ok(json)
    }

    /// Validates the complete snapshot before returning any restored state.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        if json.len() > MAX_SNAPSHOT_BYTES {
            return Err(Error::InputTooLarge);
        }
        let snapshot: Snapshot = serde_json::from_str(json).map_err(json_error)?;
        Self::restore(snapshot)
    }

    fn restore(snapshot: Snapshot) -> Result<Self, Error> {
        if snapshot.schema_version != SCHEMA_VERSION {
            return Err(Error::UnsupportedSchema(snapshot.schema_version));
        }
        if snapshot.namespace.is_nil() || snapshot.next_serial == 0 {
            return Err(Error::InvalidSnapshot("identity namespace or sequence"));
        }
        if snapshot.instances.len() > MAX_INSTANCES {
            return Err(Error::TooManyInstances);
        }
        let mut instances = BTreeMap::new();
        for instance in snapshot.instances {
            instance.preferences.validate(instance.kind)?;
            let id = instance.id;
            if id.namespace != snapshot.namespace
                || id.serial == 0
                || id.serial >= snapshot.next_serial
            {
                return Err(Error::InvalidSnapshot("instance identity"));
            }
            if instances.insert(id, instance).is_some() {
                return Err(Error::InvalidSnapshot("duplicate instance identity"));
            }
        }
        Ok(Self {
            namespace: snapshot.namespace,
            next_serial: snapshot.next_serial,
            instances,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u32,
    namespace: Uuid,
    next_serial: u64,
    instances: Vec<WidgetInstance>,
}

fn json_error(error: serde_json::Error) -> Error {
    Error::InvalidJson(error.to_string())
}

impl Serialize for WidgetStore {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.snapshot().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WidgetStore {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let snapshot = Snapshot::deserialize(deserializer)?;
        Self::restore(snapshot).map_err(serde::de::Error::custom)
    }
}
