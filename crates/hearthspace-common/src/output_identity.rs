//! Connector identity wins over legacy display roles. No Wayland or persistence I/O.

#[derive(Clone, Copy, Debug)]
pub struct SavedOutput<'a> {
    pub id: &'a str,
    pub name: &'a str,
    /// Untouched starter rows are not preferences for newly discovered connectors.
    pub placeholder: bool,
}

/// Stable order: the explicitly selected primary, then connector names.
/// Without authoritative primary metadata the order is deterministic, but callers
/// must not assign primary/secondary preferences using that fallback order.
pub fn ordered_connectors<'a>(
    connectors: &[&'a str],
    selected_primary: Option<&str>,
) -> Vec<&'a str> {
    let mut ordered = connectors.to_vec();
    ordered.sort_unstable_by(|a, b| compare_connectors(a, b, selected_primary));
    ordered.dedup();
    ordered
}

pub fn compare_connectors(a: &str, b: &str, selected_primary: Option<&str>) -> std::cmp::Ordering {
    (Some(b) == selected_primary)
        .cmp(&(Some(a) == selected_primary))
        .then_with(|| a.cmp(b))
}

pub fn legacy_role(index: usize) -> String {
    match index {
        0 => "primary".into(),
        1 => "secondary".into(),
        index => format!("secondary-{index}"),
    }
}

pub fn role_for_connector(
    connector: &str,
    connectors: &[&str],
    selected_primary: Option<&str>,
) -> Option<String> {
    let primary = selected_primary.filter(|primary| connectors.contains(primary))?;
    ordered_connectors(connectors, Some(primary))
        .iter()
        .position(|name| *name == connector)
        .map(legacy_role)
}

/// Prefer the connector ID even when an earlier legacy row has the same name.
/// A name match must be unique; role fallback is only supplied with known primary.
pub fn preference_index(
    saved: &[SavedOutput<'_>],
    connector: &str,
    role: Option<&str>,
) -> Option<usize> {
    saved
        .iter()
        .position(|saved| !saved.placeholder && saved.id == connector)
        .or_else(|| {
            let mut matches = saved
                .iter()
                .enumerate()
                .filter(|(_, saved)| !saved.placeholder && saved.name == connector);
            let (index, _) = matches.next()?;
            matches.next().is_none().then_some(index)
        })
        .or_else(|| {
            role.and_then(|role| {
                saved
                    .iter()
                    .position(|saved| !saved.placeholder && saved.id == role)
            })
        })
}

/// Resolve the connected set together so a connector/name claim cannot also be
/// reused as another output's legacy role. Results follow the input connector order.
pub fn resolve_preferences(
    saved: &[SavedOutput<'_>],
    connectors: &[&str],
    primary: Option<&str>,
) -> Vec<Option<usize>> {
    let mut matched = vec![None; connectors.len()];
    let mut used = vec![false; saved.len()];
    for (output, connector) in connectors.iter().enumerate() {
        if let Some(index) = saved
            .iter()
            .enumerate()
            .position(|(index, saved)| !used[index] && !saved.placeholder && saved.id == *connector)
        {
            matched[output] = Some(index);
            used[index] = true;
        }
    }
    for (output, connector) in connectors.iter().enumerate() {
        if matched[output].is_some() {
            continue;
        }
        let mut names = saved.iter().enumerate().filter(|(index, saved)| {
            !used[*index] && !saved.placeholder && saved.name == *connector
        });
        if let Some((index, _)) = names.next()
            && names.next().is_none()
        {
            matched[output] = Some(index);
            used[index] = true;
        }
    }
    for (output, connector) in connectors.iter().enumerate() {
        if matched[output].is_some() {
            continue;
        }
        let role = role_for_connector(connector, connectors, primary);
        if let Some(index) = saved.iter().enumerate().position(|(index, saved)| {
            !used[index] && !saved.placeholder && Some(saved.id) == role.as_deref()
        }) {
            matched[output] = Some(index);
            used[index] = true;
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_primary_and_secondary_roles_ignore_enumeration_order() {
        let first = ["HDMI-A-1", "DP-3", "eDP-1", "DP-1"];
        let reordered = ["DP-1", "eDP-1", "HDMI-A-1", "DP-3"];
        assert_eq!(
            ordered_connectors(&first, Some("eDP-1")),
            ordered_connectors(&reordered, Some("eDP-1"))
        );
        assert_eq!(
            role_for_connector("eDP-1", &reordered, Some("eDP-1")).as_deref(),
            Some("primary")
        );
        assert_eq!(
            role_for_connector("DP-1", &first, Some("eDP-1")).as_deref(),
            Some("secondary")
        );
        assert_eq!(
            role_for_connector("DP-3", &reordered, Some("eDP-1")).as_deref(),
            Some("secondary-2")
        );
    }

    #[test]
    fn unknown_or_removed_primary_does_not_assign_legacy_roles() {
        assert_eq!(role_for_connector("DP-1", &["DP-1"], None), None);
        assert_eq!(role_for_connector("DP-1", &["DP-1"], Some("eDP-1")), None);
    }

    #[test]
    fn connector_preference_wins_over_older_role_names() {
        let saved = [
            SavedOutput {
                id: "primary",
                name: "DP-1",
                placeholder: false,
            },
            SavedOutput {
                id: "DP-1",
                name: "Display",
                placeholder: false,
            },
            SavedOutput {
                id: "secondary",
                name: "Other",
                placeholder: false,
            },
        ];
        assert_eq!(preference_index(&saved, "DP-1", Some("secondary")), Some(1));
        assert_eq!(preference_index(&saved, "DP-2", Some("secondary")), Some(2));
        assert_eq!(preference_index(&saved, "DP-2", None), None);
    }

    #[test]
    fn a_named_connector_claim_cannot_be_reused_for_the_selected_primary_role() {
        let saved = [SavedOutput {
            id: "primary",
            name: "DP-1",
            placeholder: false,
        }];
        assert_eq!(
            resolve_preferences(&saved, &["eDP-1", "DP-1"], Some("eDP-1")),
            [None, Some(0)]
        );
        assert_eq!(
            resolve_preferences(&saved, &["DP-1", "eDP-1"], Some("eDP-1")),
            [Some(0), None]
        );
    }

    #[test]
    fn retained_starter_rows_never_become_new_connector_preferences() {
        let saved = [
            SavedOutput {
                id: "primary",
                name: "Primary",
                placeholder: true,
            },
            SavedOutput {
                id: "secondary",
                name: "Secondary",
                placeholder: true,
            },
            SavedOutput {
                id: "eDP-1",
                name: "Built-in",
                placeholder: false,
            },
        ];
        assert_eq!(
            resolve_preferences(&saved, &["eDP-1", "DP-1"], Some("eDP-1")),
            [Some(2), None]
        );
    }
}
