use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub type Shared = Arc<Mutex<State>>;

pub struct State {
    pub blue: bool,
    pub status: String,
    pub updated: bool,
    pub checked: bool,
    pub revision: u32,
    pub menu_only: bool,
    pub has_menu: bool,
    pub named: bool,
    pub icon_path: PathBuf,
    log: File,
}

impl State {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        Ok(Self {
            blue: false,
            status: "Active".into(),
            updated: false,
            checked: true,
            revision: 1,
            menu_only: false,
            has_menu: true,
            named: false,
            icon_path: path.with_extension("svg"),
            log: File::create_new(path)?,
        })
    }

    pub fn record(&mut self, value: serde_json::Value) {
        writeln!(self.log, "{value}").unwrap();
        self.log.flush().unwrap();
    }

    pub fn rewrite_icon(&mut self, blue: bool) -> std::io::Result<()> {
        self.named = true;
        let color = if blue { "#2060e0" } else { "#e03020" };
        std::fs::write(
            &self.icon_path,
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect x="3" y="3" width="26" height="26" fill="{color}"/></svg>"##
            ),
        )
    }

    pub fn pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        let rgb = if self.blue {
            [32, 96, 224]
        } else {
            [224, 48, 32]
        };
        let mut pixels = Vec::with_capacity(32 * 32 * 4);
        for y in 0..32 {
            for x in 0..32 {
                let alpha = if (3..29).contains(&x) && (3..29).contains(&y) {
                    255
                } else {
                    0
                };
                pixels.extend_from_slice(&[alpha, rgb[0], rgb[1], rgb[2]]);
            }
        }
        vec![(32, 32, pixels)]
    }
}
