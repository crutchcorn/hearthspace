use super::state::Shared;
use zbus::{object_server::SignalEmitter, zvariant::OwnedObjectPath};

pub struct Item(pub Shared);

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    fn activate(&self, x: i32, y: i32) {
        self.0
            .lock()
            .unwrap()
            .record(serde_json::json!({"action":"activate", "x":x, "y":y}));
    }

    fn secondary_activate(&self, x: i32, y: i32) {
        self.0
            .lock()
            .unwrap()
            .record(serde_json::json!({"action":"secondary", "x":x, "y":y}));
    }

    fn context_menu(&self, x: i32, y: i32) {
        self.0
            .lock()
            .unwrap()
            .record(serde_json::json!({"action":"context", "x":x, "y":y}));
    }

    fn scroll(&self, delta: i32, orientation: &str) {
        self.0.lock().unwrap().record(
            serde_json::json!({"action":"scroll", "delta":delta, "orientation":orientation}),
        );
    }

    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }
    #[zbus(property)]
    fn id(&self) -> &str {
        "hearthspace-tray-fixture"
    }
    #[zbus(property)]
    fn title(&self) -> &str {
        "Hearthspace Tray Fixture"
    }
    #[zbus(property)]
    fn status(&self) -> String {
        self.0.lock().unwrap().status.clone()
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
        let state = self.0.lock().unwrap();
        if state.named {
            state.icon_path.to_string_lossy().into_owned()
        } else {
            String::new()
        }
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        let state = self.0.lock().unwrap();
        if state.named { vec![] } else { state.pixmap() }
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        self.0.lock().unwrap().pixmap()
    }
    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![]
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn window_id(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        self.0.lock().unwrap().menu_only
    }
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(if self.0.lock().unwrap().has_menu {
            "/Menu"
        } else {
            "/"
        })
        .unwrap()
    }
    #[zbus(property)]
    #[allow(clippy::type_complexity)]
    fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        (
            String::new(),
            vec![],
            "Hearthspace Tray Fixture".into(),
            "Independent protocol acceptance fixture".into(),
        )
    }

    #[zbus(signal)]
    pub async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    pub async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;
}
