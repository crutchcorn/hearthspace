use super::{Placement, Rect, window_switcher_layout};

#[test]
fn switcher_centers_live_previews_and_reserves_selected_title_space() {
    let result = window_switcher_layout(
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 800.0,
        },
        &[(800.0, 600.0), (800.0, 600.0)],
    );
    assert_eq!(
        result,
        vec![
            Placement {
                x: 327.0,
                y: 274.0,
                scale: 0.36,
                matches: true
            },
            Placement {
                x: 665.0,
                y: 274.0,
                scale: 0.36,
                matches: true
            },
        ]
    );
    // The 24px badge sits 10px below a preview, within the reserved 36px.
    assert_eq!(result[0].y + 600.0 * result[0].scale + 10.0 + 24.0, 524.0);
}

#[test]
fn compact_switcher_fits_every_window_and_caption_on_an_offset_output() {
    let output = Rect {
        x: -760.0,
        y: 200.0,
        width: 760.0,
        height: 600.0,
    };
    for count in [1, 12, 100, 4096] {
        let result = window_switcher_layout(output, &vec![(640.0, 480.0); count]);
        assert_eq!(result.len(), count);
        for preview in result {
            assert!(preview.scale > 0.0 && preview.scale <= 1.0);
            assert!(preview.x >= -748.00001);
            assert!(preview.x + 640.0 * preview.scale <= -11.99999);
            assert!(preview.y >= 278.0);
            assert!(preview.y + 480.0 * preview.scale + 34.0 <= 722.00001);
        }
    }
}

#[test]
fn switcher_does_not_enlarge_small_clients_or_require_a_window() {
    let output = Rect {
        x: 100.0,
        y: 20.0,
        width: 1280.0,
        height: 800.0,
    };
    assert!(window_switcher_layout(output, &[]).is_empty());
    let result = window_switcher_layout(output, &[(80.0, 40.0)]);
    assert_eq!(result[0].scale, 1.0);
    assert_eq!(result[0].x + 40.0, output.x + output.width / 2.0);
}
