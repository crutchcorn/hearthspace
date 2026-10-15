use super::state::Shared;
use std::collections::HashMap;
use zbus::{
    object_server::SignalEmitter,
    zvariant::{OwnedValue, Value},
};

type Properties = HashMap<String, OwnedValue>;
type Layout = (i32, Properties, Vec<OwnedValue>);

fn text(value: &str) -> OwnedValue {
    Value::from(value).try_into().unwrap()
}

fn node(id: i32, label: &str, children: Vec<Layout>) -> Layout {
    let mut properties = Properties::from([
        ("label".into(), text(label)),
        ("enabled".into(), OwnedValue::from(true)),
        ("visible".into(), OwnedValue::from(true)),
    ]);
    if !children.is_empty() {
        properties.insert("children-display".into(), text("submenu"));
    }
    (
        id,
        properties,
        children
            .into_iter()
            .map(|child| Value::from(child).try_into().unwrap())
            .collect(),
    )
}

pub struct Menu(pub Shared);

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(out_args("revision", "layout"))]
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> (u32, Layout) {
        let _ = (recursion_depth, property_names);
        let state = self.0.lock().unwrap();
        let mut check = node(2, "Enabled toggle", vec![]);
        check.1.insert("toggle-type".into(), text("checkmark"));
        check.1.insert(
            "toggle-state".into(),
            OwnedValue::from(i32::from(state.checked)),
        );
        let mut disabled = node(3, "Disabled action", vec![]);
        disabled.1.insert("enabled".into(), OwnedValue::from(false));
        let mut separator = node(4, "", vec![]);
        separator.1.insert("type".into(), text("separator"));
        let nested = node(5, "More", vec![node(6, "Nested action", vec![])]);
        let layout = if parent_id == 5 {
            nested
        } else {
            node(
                0,
                "",
                vec![
                    node(
                        1,
                        if state.updated {
                            "Updated fixture"
                        } else {
                            "Open fixture"
                        },
                        vec![],
                    ),
                    check,
                    disabled,
                    separator,
                    nested,
                ],
            )
        };
        (state.revision, layout)
    }

    fn about_to_show(&self, id: i32) -> bool {
        self.0
            .lock()
            .unwrap()
            .record(serde_json::json!({"action":"about-to-show", "id":id}));
        false
    }

    async fn event(
        &self,
        id: i32,
        event_id: &str,
        data: OwnedValue,
        timestamp: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let _ = data;
        let revision = {
            let mut state = self.0.lock().unwrap();
            state.record(serde_json::json!({"action":"menu", "id":id, "event":event_id, "timestamp":timestamp}));
            if id == 2 && event_id == "clicked" {
                state.checked = !state.checked;
                state.revision += 1;
                Some(state.revision)
            } else {
                None
            }
        };
        if let Some(revision) = revision {
            Self::layout_updated(&emitter, revision, 0).await?;
        }
        Ok(())
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        vec![]
    }

    #[zbus(signal)]
    pub async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
}
