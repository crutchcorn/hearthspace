use super::ManagedWindowKind;

/// Index at which a newly raised normal window should be inserted so it sits on
/// top of every other normal window while staying below the shell bars (which
/// are kept at the end of the list).
pub(super) fn normal_insert_index_for_kinds(
    kinds: impl Iterator<Item = ManagedWindowKind>,
) -> usize {
    let mut length = 0;
    for kind in kinds {
        if kind.is_shell_chrome() {
            return length;
        }
        length += 1;
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_index_keeps_normal_windows_below_shell_chrome() {
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
            1,
            "an interleaved normal must not pull more windows above the shell"
        );
    }
}
