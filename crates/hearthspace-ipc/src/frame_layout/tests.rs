use super::*;
use hearthspace_common::frame_layout::{ClockDateFormat, LayoutTarget, ObjectPlacement};

#[test]
fn editor_actions_have_one_shared_wire_contract_and_required_payloads() {
    let target = LayoutTarget::widget("clock");
    for action in [
        Action::Done,
        Action::Cancel,
        Action::Confirm,
        Action::AllowHideTools { value: true },
        Action::Select {
            target: target.clone(),
        },
        Action::Attach {
            target: target.clone(),
        },
        Action::Place {
            target: target.clone(),
            placement: ObjectPlacement::canvas(),
        },
        Action::Remove {
            target: target.clone(),
        },
        Action::Ungroup {
            target: target.clone(),
        },
        Action::ToggleExpanded {
            target: target.clone(),
        },
        Action::ClockDateFormat {
            target: target.clone(),
            value: ClockDateFormat::Long,
        },
    ] {
        let request = Command {
            session: 17,
            output: "Virtual output 2".into(),
            action,
        };
        let bytes = serde_json::to_vec(&request).unwrap();
        assert_eq!(serde_json::from_slice::<Command>(&bytes).unwrap(), request);
    }
    for invalid in [
        r#"{"session":17,"output":"eDP-1","action":"select"}"#,
        r#"{"session":17,"output":"eDP-1","action":"place","target":{"kind":"widget","id":"clock"}}"#,
        r#"{"session":17,"output":"eDP-1","action":"allow-hide-tools","value":"yes"}"#,
        r#"{"session":17,"output":"eDP-1","action":"clock-date-format","target":{"kind":"widget","id":"clock"},"value":"tomorrow"}"#,
        r#"{"session":17,"output":"eDP-1","action":"unknown"}"#,
    ] {
        assert!(
            serde_json::from_str::<Command>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn confirmation_payload_cannot_describe_close_without_a_window() {
    assert!(serde_json::from_str::<Confirmation>(r#"{"kind":"close-window"}"#).is_err());
    let confirmation = Confirmation::CloseWindow { window_id: 27 };
    let bytes = serde_json::to_vec(&confirmation).unwrap();
    assert_eq!(
        serde_json::from_slice::<Confirmation>(&bytes).unwrap(),
        confirmation
    );
}
