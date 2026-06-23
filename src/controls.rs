use smithay::utils::{Logical, Rectangle};

use crate::config::{BUTTON_GAP, BUTTON_HEIGHT, BUTTON_Y};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlAction {
    SpawnApp,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    ZoomIn,
    ZoomOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlButton {
    pub action: ControlAction,
    pub rect: Rectangle<i32, Logical>,
}

pub fn control_buttons() -> Vec<ControlButton> {
    let specs = [
        (ControlAction::SpawnApp, 112),
        (ControlAction::PanLeft, 64),
        (ControlAction::PanRight, 64),
        (ControlAction::PanUp, 64),
        (ControlAction::PanDown, 64),
        (ControlAction::ZoomIn, 88),
        (ControlAction::ZoomOut, 88),
    ];
    let mut x = BUTTON_GAP;

    specs
        .into_iter()
        .map(|(action, width)| {
            let rect = Rectangle::new((x, BUTTON_Y).into(), (width, BUTTON_HEIGHT).into());
            x += width + BUTTON_GAP;
            ControlButton { action, rect }
        })
        .collect()
}

pub fn label_rects(button: ControlButton) -> Vec<Rectangle<i32, Logical>> {
    const SCALE: i32 = 2;
    const GLYPH_WIDTH: i32 = 5;
    const GLYPH_HEIGHT: i32 = 7;
    const LETTER_GAP: i32 = 1;

    let label = match button.action {
        ControlAction::SpawnApp => "SPAWN",
        ControlAction::PanLeft => "LEFT",
        ControlAction::PanRight => "RIGHT",
        ControlAction::PanUp => "UP",
        ControlAction::PanDown => "DOWN",
        ControlAction::ZoomIn => "ZOOM+",
        ControlAction::ZoomOut => "ZOOM-",
    };

    let char_count = label.chars().count() as i32;
    let label_width = (char_count * GLYPH_WIDTH + (char_count - 1) * LETTER_GAP) * SCALE;
    let label_height = GLYPH_HEIGHT * SCALE;
    let start_x = button.rect.loc.x + (button.rect.size.w - label_width) / 2;
    let start_y = button.rect.loc.y + (button.rect.size.h - label_height) / 2;
    let mut rects = Vec::new();

    for (char_index, ch) in label.chars().enumerate() {
        let glyph_x = start_x + char_index as i32 * (GLYPH_WIDTH + LETTER_GAP) * SCALE;

        for (row, pattern) in glyph_pattern(ch).iter().enumerate() {
            for (col, pixel) in pattern.bytes().enumerate() {
                if pixel == b'#' {
                    rects.push(Rectangle::new(
                        (glyph_x + col as i32 * SCALE, start_y + row as i32 * SCALE).into(),
                        (SCALE, SCALE).into(),
                    ));
                }
            }
        }
    }

    rects
}

pub fn glyph_pattern(ch: char) -> [&'static str; 7] {
    match ch {
        'A' => [
            ".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
        'D' => [
            "####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####.",
        ],
        'E' => [
            "#####", "#....", "#....", "####.", "#....", "#....", "#####",
        ],
        'F' => [
            "#####", "#....", "#....", "####.", "#....", "#....", "#....",
        ],
        'G' => [
            ".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".###.",
        ],
        'H' => [
            "#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
        'I' => [
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####",
        ],
        'L' => [
            "#....", "#....", "#....", "#....", "#....", "#....", "#####",
        ],
        'N' => [
            "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#",
        ],
        'O' => [
            ".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
        'P' => [
            "####.", "#...#", "#...#", "####.", "#....", "#....", "#....",
        ],
        'R' => [
            "####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#",
        ],
        'S' => [
            ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
        ],
        'T' => [
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
        ],
        'U' => [
            "#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
        'W' => [
            "#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#",
        ],
        'Z' => [
            "#####", "....#", "...#.", "..#..", ".#...", "#....", "#####",
        ],
        'M' => [
            "#...#", "##.##", "#.#.#", "#...#", "#...#", "#...#", "#...#",
        ],
        '+' => [
            ".....", "..#..", "..#..", "#####", "..#..", "..#..", ".....",
        ],
        '-' => [
            ".....", ".....", ".....", "#####", ".....", ".....", ".....",
        ],
        _ => [
            ".....", ".....", ".....", ".....", ".....", ".....", ".....",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_buttons_are_laid_out_in_order() {
        let buttons = control_buttons();

        assert_eq!(buttons.len(), 7);
        assert_eq!(buttons[0].action, ControlAction::SpawnApp);
        assert_eq!(buttons[0].rect.loc.x, BUTTON_GAP);
        assert_eq!(buttons[1].action, ControlAction::PanLeft);
        assert!(buttons
            .windows(2)
            .all(|pair| pair[0].rect.loc.x < pair[1].rect.loc.x));
    }

    #[test]
    fn label_rects_stay_inside_button_bounds() {
        for button in control_buttons() {
            let rects = label_rects(button);
            assert!(!rects.is_empty());
            assert!(rects.iter().all(|rect| rect.loc.x >= button.rect.loc.x));
            assert!(rects.iter().all(|rect| rect.loc.y >= button.rect.loc.y));
            assert!(rects
                .iter()
                .all(|rect| rect.loc.x + rect.size.w <= button.rect.loc.x + button.rect.size.w));
            assert!(rects
                .iter()
                .all(|rect| rect.loc.y + rect.size.h <= button.rect.loc.y + button.rect.size.h));
        }
    }

    #[test]
    fn unknown_glyph_is_blank() {
        assert_eq!(glyph_pattern('?'), ["....."; 7]);
    }
}
