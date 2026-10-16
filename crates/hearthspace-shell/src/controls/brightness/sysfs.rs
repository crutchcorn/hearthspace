use std::path::Path;

use super::BrightnessStatus;

#[derive(Debug)]
pub(super) struct Device {
    pub name: String,
    maximum: u32,
    current: u32,
    preference: u8,
}

impl Device {
    pub fn status(&self) -> BrightnessStatus {
        BrightnessStatus {
            percentage: ((u64::from(self.current) * 100 + u64::from(self.maximum) / 2)
                / u64::from(self.maximum)) as u8,
            device: self.name.clone(),
        }
    }

    pub fn level(&self, percentage: u8) -> u32 {
        ((u64::from(self.maximum) * u64::from(percentage.clamp(1, 100)) + 50) / 100).max(1) as u32
    }
}

pub(super) async fn discover(root: &Path) -> Result<Option<Device>, String> {
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Display brightness devices could not be read.".into()),
    };
    let mut devices = Vec::new();
    let mut invalid = false;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| "Display brightness devices could not be read.")?
    {
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| valid_name(name)) else {
            continue;
        };
        match read(root, name).await {
            Ok(device) => devices.push(device),
            Err(_) => invalid = true,
        }
    }
    // Firmware/platform interfaces can mediate the same hardware as a raw
    // interface. Prefer those, then use a stable name for deterministic choice.
    devices.sort_by(|a, b| (a.preference, &a.name).cmp(&(b.preference, &b.name)));
    if devices.is_empty() && invalid {
        Err("Display brightness is not reporting a valid level.".into())
    } else {
        Ok(devices.into_iter().next())
    }
}

pub(super) async fn read(root: &Path, name: &str) -> Result<Device, String> {
    if !valid_name(name) {
        return Err("Invalid display brightness device.".into());
    }
    let directory = root.join(name);
    let read = |file| tokio::fs::read_to_string(directory.join(file));
    let maximum = read("max_brightness")
        .await
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .ok_or("Display brightness has no valid maximum.")?;
    // The kernel ABI recommends `brightness` for userspace decisions:
    // `actual_brightness` can also reflect blanking or power-saving state.
    let current = read("brightness")
        .await
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value <= maximum)
        .ok_or("Display brightness has no valid current level.")?;
    let kind = read("type")
        .await
        .map_err(|_| "Display brightness has no device type.")?;
    let preference = match kind.trim() {
        "firmware" => 0,
        "platform" => 1,
        "raw" => 2,
        _ => return Err("Display brightness has an unknown device type.".into()),
    };
    Ok(Device {
        name: name.into(),
        maximum,
        current,
        preference,
    })
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}
