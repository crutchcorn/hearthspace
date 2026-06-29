use super::ManagedWindowKind;

/// Index at which a newly raised normal window should be inserted so it sits on
/// top of every other normal window while staying below the shell bars (which
/// are kept at the end of the list).
pub(super) fn normal_insert_index_for_kinds(
    kinds: impl Iterator<Item = ManagedWindowKind>,
) -> usize {
    kinds
        .enumerate()
        .filter_map(|(index, kind)| (kind == ManagedWindowKind::Normal).then_some(index + 1))
        .last()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_index_is_after_the_last_normal_window() {
        use ManagedWindowKind::{Normal, ShellBar};
        assert_eq!(normal_insert_index_for_kinds([].into_iter()), 0);
        assert_eq!(
            normal_insert_index_for_kinds([ShellBar].into_iter()),
            0,
            "with only shell bars, normals go to the front"
        );
        assert_eq!(
            normal_insert_index_for_kinds([Normal, Normal, ShellBar].into_iter()),
            2,
            "insert above the topmost normal but below the shell bar"
        );
        assert_eq!(
            normal_insert_index_for_kinds([Normal, ShellBar, Normal].into_iter()),
            3
        );
    }
}
