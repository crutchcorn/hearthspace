use super::*;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

#[derive(Default)]
pub(super) struct State {
    pub values: BTreeMap<String, String>,
    pub reads: usize,
    pub set_calls: Vec<BTreeMap<String, String>>,
    pub replace_calls: Vec<(Vec<String>, BTreeMap<String, String>)>,
    pub fail_next_set: bool,
    pub stall_reads: bool,
    pub set_gate: Option<oneshot::Receiver<()>>,
    pub change_on_read: Option<(usize, BTreeMap<String, String>)>,
    pub fail_replace_after: Option<usize>,
}

pub(super) struct Manager(pub Arc<Mutex<State>>);

fn assignments(entries: &[String]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|entry| {
            let (key, value) = entry.split_once('=').expect("manager assignment");
            (key.to_owned(), value.to_owned())
        })
        .collect()
}

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    #[zbus(property)]
    async fn environment(&self) -> Vec<String> {
        let stall = {
            let mut state = self.0.lock().unwrap();
            state.reads += 1;
            if state
                .change_on_read
                .as_ref()
                .is_some_and(|(read, _)| *read == state.reads)
            {
                let (_, values) = state.change_on_read.take().unwrap();
                state.values.extend(values);
            }
            state.stall_reads
        };
        if stall {
            std::future::pending::<()>().await;
        }
        self.0
            .lock()
            .unwrap()
            .values
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect()
    }

    async fn set_environment(&self, entries: Vec<String>) -> zbus::fdo::Result<()> {
        let values = assignments(&entries);
        let (fail, gate) = {
            let mut state = self.0.lock().unwrap();
            state.set_calls.push(values.clone());
            (
                std::mem::take(&mut state.fail_next_set),
                state.set_gate.take(),
            )
        };
        if let Some(gate) = gate {
            let _ = gate.await;
        }
        if fail {
            return Err(zbus::fdo::Error::Failed(
                "injected SetEnvironment failure".into(),
            ));
        }
        self.0.lock().unwrap().values.extend(values);
        Ok(())
    }

    fn unset_and_set_environment(
        &self,
        unset: Vec<String>,
        set: Vec<String>,
    ) -> zbus::fdo::Result<()> {
        let set = assignments(&set);
        let mut state = self.0.lock().unwrap();
        state.replace_calls.push((unset.clone(), set.clone()));
        if state
            .fail_replace_after
            .is_some_and(|count| state.replace_calls.len() > count)
        {
            return Err(zbus::fdo::Error::Failed(
                "injected retirement failure".into(),
            ));
        }
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
        Ok(())
    }
}
