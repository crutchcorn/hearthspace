//! Temporary screen-space cards. No canvas position or client size is changed.
use smithay::utils::{Logical, Point, Rectangle, Size};

pub(super) const HEADER_HEIGHT: i32 = 88;
pub(super) const LABEL_HEIGHT: i32 = 32;

#[derive(Debug, Clone)]
pub(in crate::compositor) struct Item {
    pub(in crate::compositor) id: u64,
    pub(in crate::compositor) rect: Rectangle<i32, Logical>,
    pub(in crate::compositor) preview: Rectangle<i32, Logical>,
    pub(in crate::compositor) label: Rectangle<i32, Logical>,
    pub(in crate::compositor) scale: f64,
}

pub(super) fn arrange(
    output: Rectangle<i32, Logical>,
    windows: &[(u64, Size<i32, Logical>)],
) -> Vec<Item> {
    if windows.is_empty() {
        return Vec::new();
    }
    let margin = 32.min(output.size.w / 10).min(output.size.h / 10).max(0);
    let header = HEADER_HEIGHT.min(output.size.h / 4).max(0);
    let width = (output.size.w - margin * 2).max(1);
    let height = (output.size.h - header - margin * 2).max(1);
    let columns = ((windows.len() as f64 * f64::from(width) / f64::from(height) / 1.6)
        .sqrt()
        .ceil() as usize)
        .clamp(1, windows.len());
    let rows = windows.len().div_ceil(columns);
    let gap = 24
        .min(width / columns as i32 / 5)
        .min(height / rows as i32 / 5);
    let card_w = ((width - gap * (columns as i32 - 1)) / columns as i32).clamp(1, 720);
    let card_h = ((height - gap * (rows as i32 - 1)) / rows as i32).clamp(1, 480);
    let top = output.loc.y
        + header
        + margin
        + (height - rows as i32 * card_h - (rows as i32 - 1) * gap) / 2;
    windows
        .iter()
        .enumerate()
        .map(|(index, (id, size))| {
            let row = index / columns;
            let row_count = (windows.len() - row * columns).min(columns);
            let left = output.loc.x
                + margin
                + (width - row_count as i32 * card_w - (row_count as i32 - 1) * gap) / 2;
            let rect = Rectangle::new(
                (
                    left + (index % columns) as i32 * (card_w + gap),
                    top + row as i32 * (card_h + gap),
                )
                    .into(),
                (card_w, card_h).into(),
            );
            card(*id, rect, *size)
        })
        .collect()
}

pub(in crate::compositor) fn card(
    id: u64,
    rect: Rectangle<i32, Logical>,
    size: Size<i32, Logical>,
) -> Item {
    let card_w = rect.size.w;
    let card_h = rect.size.h;
    let inset = 12.min(card_w / 8).min(card_h / 8);
    let label_h = LABEL_HEIGHT.min((card_h / 4).max(1));
    let available = Size::<i32, Logical>::from((
        (card_w - inset * 2).max(1),
        (card_h - inset * 2 - label_h).max(1),
    ));
    let scale = (f64::from(available.w) / f64::from(size.w.max(1)))
        .min(f64::from(available.h) / f64::from(size.h.max(1)))
        .min(1.0);
    let preview_size = Size::from((
        ((f64::from(size.w.max(1)) * scale).floor() as i32).max(1),
        ((f64::from(size.h.max(1)) * scale).floor() as i32).max(1),
    ));
    let preview = Rectangle::new(
        rect.loc
            + Point::from((
                (card_w - preview_size.w) / 2,
                inset + (available.h - preview_size.h) / 2,
            )),
        preview_size,
    );
    Item {
        id,
        rect,
        preview,
        label: Rectangle::new(
            rect.loc + Point::from((inset, card_h - inset - label_h)),
            (available.w, label_h).into(),
        ),
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_fit_each_output_without_overlap_and_preserve_identity() {
        for count in [1, 2, 3, 5, 9, 27, 100] {
            for output in [
                Rectangle::new((1920, -200).into(), (1280, 800).into()),
                Rectangle::from_size((600, 1024).into()),
            ] {
                let windows: Vec<_> = (0..count)
                    .map(|id| (id, (320 + id as i32 * 30, 480).into()))
                    .collect();
                let items = arrange(output, &windows);
                assert_eq!(items.len(), windows.len());
                for (index, item) in items.iter().enumerate() {
                    assert_eq!(item.id, windows[index].0);
                    assert!(output.contains_rect(item.rect));
                    assert!(item.rect.contains_rect(item.preview));
                    assert!(item.rect.contains_rect(item.label));
                    assert!(item.scale > 0.0 && item.scale <= 1.0);
                    for other in &items[index + 1..] {
                        assert!(!item.rect.overlaps(other.rect));
                    }
                }
            }
        }
    }

    #[test]
    fn thumbnails_keep_aspect_ratio_and_do_not_enlarge_small_windows() {
        let output = Rectangle::from_size((1280, 800).into());
        let items = arrange(output, &[(8, (1600, 900).into()), (2, (100, 200).into())]);
        assert!(
            (f64::from(items[0].preview.size.w) / f64::from(items[0].preview.size.h)
                - 1600.0 / 900.0)
                .abs()
                < 0.02
        );
        assert_eq!(items[1].scale, 1.0);
        assert_eq!(items[1].preview.size, (100, 200).into());
    }

    #[test]
    fn empty_and_tiny_outputs_are_safe() {
        assert!(arrange(Rectangle::from_size((1, 1).into()), &[]).is_empty());
        let tiny = arrange(Rectangle::from_size((1, 1).into()), &[(1, (0, 0).into())]);
        assert_eq!(tiny.len(), 1);
        assert!(tiny[0].scale.is_finite());
    }
}
