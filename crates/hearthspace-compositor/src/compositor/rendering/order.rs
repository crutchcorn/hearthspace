use crate::compositor::ManagedWindowKind;

/// Shell surfaces form a separate top layer. Never let an unrelated normal
/// window's mapping order put the bar below a fullscreen rendering cutoff.
pub(super) fn render_order(kinds: impl IntoIterator<Item = ManagedWindowKind>) -> Vec<usize> {
    let (mut shell, mut normal) = (Vec::new(), Vec::new());
    for (index, kind) in kinds.into_iter().enumerate() {
        if kind.is_shell_chrome() {
            shell.push(index);
        } else {
            normal.push(index);
        }
    }
    shell.reverse();
    normal.reverse();
    shell.extend(normal);
    shell
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleaved_shell_survives_a_fullscreen_cutoff() {
        use ManagedWindowKind::{Normal, ShellBar};
        // An override-redirect map can arrive after shell chrome. A floating
        // app (3) above fullscreen (2) must still leave the bar (1) visible.
        let order = render_order([Normal, ShellBar, Normal, Normal]);
        let mut drawn = Vec::new();
        for index in order {
            drawn.push(index);
            if index == 2 {
                break;
            }
        }
        assert_eq!(drawn, [1, 3, 2]);
    }

    #[test]
    fn shell_layer_preserves_both_normal_and_shell_stacking() {
        use ManagedWindowKind::{Launcher, Normal, ShellBar};
        assert_eq!(
            render_order([Normal, ShellBar, Normal, Launcher]),
            [3, 1, 2, 0]
        );
    }
}
