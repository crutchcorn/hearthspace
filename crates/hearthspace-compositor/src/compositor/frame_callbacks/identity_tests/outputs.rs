use super::*;
use crate::compositor::OutputDescriptor;
use smithay::output::{PhysicalProperties, Subpixel};

fn outputs(f: &mut AppFixture, names: &[&str]) {
    let descriptors = names
        .iter()
        .map(|name| OutputDescriptor {
            name: (*name).into(),
            properties: PhysicalProperties {
                size: (300, 200).into(),
                subpixel: Subpixel::Unknown,
                make: "Identity test".into(),
                model: (*name).into(),
                serial_number: (*name).into(),
            },
            size: (800, 600).into(),
            scale: 1,
            refresh: 60_000,
        })
        .collect();
    f.app
        .sync_connector_outputs(&f.app.display_handle.clone(), descriptors);
    // Geometry is fixture input, as in the existing callback geometry tests.
    // Identity, connector promotion, visibility and callback dispatch are real.
    f.app.layout_editor.placed = f
        .app
        .capture_outputs()
        .iter()
        .flat_map(|output| {
            ["wifi", "clock"].map(|id| PlacedTarget {
                target: Target {
                    output: output.name.clone(),
                    target: LayoutTarget::widget(id),
                },
                rect: Rect {
                    x: f64::from(output.logical_x + 20),
                    y: 20.,
                    width: 44.,
                    height: 44.,
                },
                placement: ObjectPlacement::canvas(),
                group: None,
                vertical: false,
            })
        })
        .collect();
}

#[test]
fn cached_outputless_frame_identity_follows_current_primary_and_output_reappearance() {
    let mut f = AppFixture::new();
    let geometry = Rectangle::new((0, 0).into(), (44, 44).into());
    let widget = f.window(ManagedWindowKind::ShellBar, None, geometry);
    let group = f.window(ManagedWindowKind::ShellBar, None, geometry);
    let grip = f.window(ManagedWindowKind::ShellBar, None, geometry);
    let explicit = f.window(ManagedWindowKind::ShellBar, Some("A"), geometry);
    f.set_app_id(widget, &frame_widget_app_id(None, "wifi"));
    f.set_app_id(group, &frame_group_app_id(None, "system"));
    f.set_app_id(grip, &frame_group_grip_app_id(None, "system"));
    f.set_app_id(explicit, &frame_widget_app_id(Some("A"), "clock"));

    for (names, primary, time) in [
        (vec!["A", "B"], Some("A"), 10),
        (vec!["B", "A"], Some("B"), 20),
        (vec![], None, 30),
        (vec!["B"], Some("B"), 40),
        (vec!["B", "A"], Some("B"), 50),
    ] {
        outputs(&mut f, &names);
        for index in [widget, group, grip] {
            assert!(f.app.is_layout_surface(index));
            let expected = primary.map(|output| Target {
                output: output.into(),
                target: if index == widget {
                    LayoutTarget::widget("wifi")
                } else {
                    LayoutTarget::new(TargetKind::Group, "system")
                },
            });
            assert_eq!(f.app.frame_target_for_window(index), expected);
        }
        // A grip is still syntactically a grip with no resolvable output.
        assert!(f.app.windows[grip].is_frame_grip());
        assert_eq!(
            f.app.frame_widget_identity(explicit),
            Some(("A".into(), "clock".into()))
        );
        assert_eq!(f.app.layout_surface_visible(explicit), names.contains(&"A"));
        assert_eq!(f.app.layout_surface_visible(widget), primary.is_some());

        // Drain every request only on its live cadence. With no outputs the
        // callbacks remain queued until a real connector reappears.
        if time != 40 {
            f.request_frame(widget);
            f.request_frame(explicit);
        }
        let (on_a, on_b) = match names.as_slice() {
            ["A", "B"] => (vec![(widget, time), (explicit, time)], vec![]),
            ["B", "A"] => (vec![(explicit, time)], vec![(widget, time + 1)]),
            ["B"] => (vec![], vec![(widget, time + 1), (explicit, time + 1)]),
            [] => (vec![], vec![]),
            _ => unreachable!(),
        };
        assert_eq!(f.send_output("A", time), on_a.len());
        f.assert_done(&on_a);
        assert_eq!(f.send_output("B", time + 1), on_b.len());
        f.assert_done(&on_b);
    }
}
