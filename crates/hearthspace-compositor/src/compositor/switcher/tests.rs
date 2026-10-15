use smithay::utils::Rectangle;

use super::{layout, surviving_selection};

#[test]
fn removal_keeps_stable_order_and_selects_the_next_survivor() {
    let mut ids = vec![2, 4, 7, 10];
    assert_eq!(surviving_selection(&mut ids, 4, &[10, 2, 7, 99]), Some(7));
    assert_eq!(ids, vec![2, 7, 10]);
    assert_eq!(surviving_selection(&mut ids, 10, &[2, 7]), Some(2));
    assert_eq!(surviving_selection(&mut ids, 2, &[]), None);
    assert!(ids.is_empty());
}

#[test]
fn live_selection_does_not_follow_z_order_or_new_clients() {
    let mut ids = vec![2, 4, 7];
    assert_eq!(surviving_selection(&mut ids, 4, &[7, 4, 99, 2]), Some(4));
    assert_eq!(ids, vec![2, 4, 7]);
}

#[test]
fn strip_keeps_selected_card_visible_and_preserves_canvas_geometry() {
    for output in [
        Rectangle::new((1280, -80).into(), (1280, 800).into()),
        Rectangle::from_size((480, 800).into()),
        Rectangle::from_size((180, 160).into()),
    ] {
        for count in [1, 3, 8, 50] {
            let windows: Vec<_> = (0..count).map(|id| (id, (1600, 900).into())).collect();
            for selected in 0..count {
                let strip = layout::arrange(output, &windows, selected);
                assert!(output.contains_rect(strip.panel));
                assert!(strip.items.iter().any(|item| item.id == selected));
                assert!(strip.items.len() <= windows.len());
                for (index, item) in strip.items.iter().enumerate() {
                    assert!(strip.panel.contains_rect(item.rect));
                    assert!(item.rect.contains_rect(item.preview));
                    assert!(item.rect.contains_rect(item.label));
                    assert!((0.0..=1.0).contains(&item.scale));
                    for other in &strip.items[index + 1..] {
                        assert!(!item.rect.overlaps(other.rect));
                    }
                }
            }
        }
    }
}
