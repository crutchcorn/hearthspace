use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const MARKER: &str = "HEARTHSPACE_ACTIVATION_OWNER";

#[derive(Default)]
pub struct State {
    pub values: BTreeMap<String, String>,
    pub events: Vec<&'static str>,
    pub set_calls: Vec<BTreeMap<String, String>>,
    pub replacements: Vec<BTreeMap<String, String>>,
    pub fail_reads: bool,
}

pub struct Manager(pub Arc<Mutex<State>>);

fn assignments(entries: Vec<String>) -> BTreeMap<String, String> {
    entries
        .into_iter()
        .map(|entry| {
            let (key, value) = entry.split_once('=').expect("manager assignment");
            (key.to_owned(), value.to_owned())
        })
        .collect()
}

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    #[zbus(property)]
    fn environment(&self) -> zbus::fdo::Result<Vec<String>> {
        let mut state = self.0.lock().unwrap();
        if state.fail_reads {
            state.events.push("read-failed");
            return Err(zbus::fdo::Error::Failed(
                "injected Environment failure".into(),
            ));
        }
        state.events.push("read");
        Ok(state
            .values
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect())
    }

    fn set_environment(&self, entries: Vec<String>) -> zbus::fdo::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.events.push("set-failed");
        state.set_calls.push(assignments(entries));
        // dbus-update already updated the broker. Keep the manager's claimed
        // ownership but make every rollback/recovery read fail until repaired.
        state.fail_reads = true;
        Err(zbus::fdo::Error::Failed(
            "injected SetEnvironment failure".into(),
        ))
    }

    fn unset_and_set_environment(&self, unset: Vec<String>, set: Vec<String>) {
        let mut state = self.0.lock().unwrap();
        let set = assignments(set);
        state.events.push("replace");
        state.replacements.push(set.clone());
        for entry in unset {
            if let Some((key, value)) = entry.split_once('=') {
                if state.values.get(key).map(String::as_str) == Some(value) {
                    state.values.remove(key);
                }
            } else {
                state.values.remove(&entry);
            }
        }
        state.values.extend(set);
    }
}
