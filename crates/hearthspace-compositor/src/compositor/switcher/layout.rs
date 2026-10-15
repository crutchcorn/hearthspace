use smithay::utils::{Logical, Rectangle, Size};

use crate::compositor::overview::layout::{Item, card};

pub(super) struct Strip {
    pub(super) panel: Rectangle<i32, Logical>,
    pub(super) caption: Rectangle<i32, Logical>,
    pub(super) items: Vec<Item>,
}

pub(super) fn arrange(
    output: Rectangle<i32, Logical>,
    windows: &[(u64, Size<i32, Logical>)],
    selected: u64,
) -> Strip {
    let margin = 24.min(output.size.w / 8).min(output.size.h / 8).max(0);
    let available_w = (output.size.w - margin * 2).max(1);
    let available_h = (output.size.h - margin * 2).max(1);
    let padding = 16.min(available_w / 8).min(available_h / 8);
    let gap = 12.min(available_w / 12);
    let caption_h = 30.min(available_h / 5);
    let card_w = 184.min((available_w - 2 * padding).max(1));
    let card_h = 164.min((available_h - 2 * padding - caption_h).max(1));
    let capacity = ((available_w - 2 * padding + gap) / (card_w + gap)).max(1) as usize;
    let count = windows.len().min(capacity);
    let selected_index = windows
        .iter()
        .position(|(id, _)| *id == selected)
        .unwrap_or(0);
    let start = selected_index
        .saturating_sub(count / 2)
        .min(windows.len() - count);
    let width = (count as i32 * card_w + count.saturating_sub(1) as i32 * gap + padding * 2).max(1);
    let height = card_h + caption_h + padding * 2;
    let panel = Rectangle::new(
        (
            output.loc.x + (output.size.w - width) / 2,
            output.loc.y + (output.size.h - height) / 2,
        )
            .into(),
        (width, height).into(),
    );
    let items = windows
        .iter()
        .skip(start)
        .take(count)
        .enumerate()
        .map(|(index, (id, size))| {
            card(
                *id,
                Rectangle::new(
                    (
                        panel.loc.x + padding + index as i32 * (card_w + gap),
                        panel.loc.y + padding,
                    )
                        .into(),
                    (card_w, card_h).into(),
                ),
                *size,
            )
        })
        .collect();
    Strip {
        panel,
        caption: Rectangle::new(
            (panel.loc.x + padding, panel.loc.y + padding + card_h).into(),
            ((width - 2 * padding).max(1), caption_h.max(1)).into(),
        ),
        items,
    }
}
