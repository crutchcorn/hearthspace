use super::surviving_selection;

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
