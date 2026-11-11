//! Cache what assertions observed; capturing after client teardown loses the failure.
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    sync::Mutex,
};

#[derive(Default)]
pub(super) struct Diagnostics {
    frame: Mutex<Option<Vec<u8>>>,
    commands: Mutex<VecDeque<serde_json::Value>>,
}

impl Diagnostics {
    pub(super) fn command(&self, command: &str, reply: &str) {
        let mut recent = self.commands.lock().unwrap_or_else(|e| e.into_inner());
        if recent.len() == 32 {
            recent.pop_front();
        }
        recent.push_back(serde_json::json!({
            "command": command.chars().take(1024).collect::<String>(),
            "reply": reply.chars().take(8192).collect::<String>(),
        }));
    }

    pub(super) fn frame(&self, bytes: &[u8]) {
        *self.frame.lock().unwrap_or_else(|e| e.into_inner()) = Some(bytes.to_vec());
    }

    pub(super) fn persist(&self, directory: &Path) -> io::Result<()> {
        if let Some(frame) = self
            .frame
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            std::fs::write(directory.join("last-observed.png"), frame)?;
        }
        let commands =
            serde_json::to_vec_pretty(&*self.commands.lock().unwrap_or_else(|e| e.into_inner()))?;
        std::fs::write(directory.join("control-tail.json"), commands)
    }
}

pub(super) fn log_tail(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(16 * 1024);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(16 * 1024).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
