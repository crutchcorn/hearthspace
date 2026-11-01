use super::*;

mod service;

fn device(default: bool, discrete: Option<bool>, values: &[&str]) -> HashMap<String, OwnedValue> {
    let mut properties = HashMap::from([
        ("Default".into(), OwnedValue::from(default)),
        (
            "Environment".into(),
            OwnedValue::try_from(zbus::zvariant::Value::from(
                values
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>(),
            ))
            .unwrap(),
        ),
    ]);
    if let Some(discrete) = discrete {
        properties.insert("Discrete".into(), OwnedValue::from(discrete));
    }
    properties
}

#[test]
fn a_discrete_default_needs_no_offload_even_with_another_discrete_gpu() {
    assert!(
        select_environment(vec![
            device(false, Some(true), &["DRI_PRIME", "secondary"]),
            device(true, Some(true), &["DRI_PRIME", "default"]),
        ])
        .unwrap()
        .is_empty()
    );
}

#[test]
fn first_discrete_precedes_an_earlier_non_default_integrated_gpu() {
    assert_eq!(
        select_environment(vec![
            device(true, Some(false), &[]),
            device(false, Some(false), &["DRI_PRIME", "integrated"]),
            device(false, Some(true), &["DRI_PRIME", "discrete"]),
            device(false, Some(true), &["DRI_PRIME", "later"]),
        ])
        .unwrap(),
        [("DRI_PRIME".into(), "discrete".into())]
    );
}

#[test]
fn older_inventory_uses_first_non_default_and_a_single_default_stays_ordinary() {
    assert_eq!(
        select_environment(vec![
            device(true, None, &[]),
            device(false, None, &["DRI_PRIME", "1"]),
            device(false, None, &["DRI_PRIME", "2"]),
        ])
        .unwrap(),
        [("DRI_PRIME".into(), "1".into())]
    );
    assert!(
        select_environment(vec![device(true, None, &[])])
            .unwrap()
            .is_empty()
    );
    assert!(select_environment(Vec::new()).unwrap().is_empty());
}

#[test]
fn malformed_environment_is_rejected_as_a_whole() {
    for values in [
        vec!["DRI_PRIME", "1", "missing-value"],
        vec!["BAD=KEY", "value"],
        vec!["DRI_PRIME", "1", "DRI_PRIME", "2"],
        vec!["DRI_PRIME", "bad\0value"],
    ] {
        assert!(select_environment(vec![device(false, None, &values)]).is_err());
    }
}
