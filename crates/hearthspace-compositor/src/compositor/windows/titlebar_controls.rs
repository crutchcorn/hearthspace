//! One logical geometry definition for rasterized controls and input.
use smithay::utils::{Logical, Rectangle};

use crate::config::{
    CLOSE_BUTTON_MARGIN, CLOSE_BUTTON_SIZE, TITLE_BAR_BUTTON_GAP, TITLE_BAR_HEIGHT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::compositor) enum TitlebarButton {
    Maximize,
    Close,
}

pub(in crate::compositor) fn titlebar_button_rect(
    width: i32,
    button: TitlebarButton,
) -> Rectangle<i32, Logical> {
    let offset = match button {
        TitlebarButton::Maximize => CLOSE_BUTTON_SIZE + TITLE_BAR_BUTTON_GAP,
        TitlebarButton::Close => 0,
    };
    Rectangle::new(
        (
            width - CLOSE_BUTTON_MARGIN - CLOSE_BUTTON_SIZE - offset,
            (TITLE_BAR_HEIGHT - CLOSE_BUTTON_SIZE) / 2,
        )
            .into(),
        (CLOSE_BUTTON_SIZE, CLOSE_BUTTON_SIZE).into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_controls_share_the_exact_header_geometry() {
        assert_eq!(
            titlebar_button_rect(678, TitlebarButton::Maximize),
            Rectangle::new((604, 6).into(), (32, 32).into())
        );
        assert_eq!(
            titlebar_button_rect(678, TitlebarButton::Close),
            Rectangle::new((640, 6).into(), (32, 32).into())
        );
        for button in [TitlebarButton::Maximize, TitlebarButton::Close] {
            let rect = titlebar_button_rect(crate::config::MIN_WINDOW_WIDTH, button);
            assert!(rect.loc.x >= 0 && rect.loc.y >= 0);
            assert!(rect.loc.y + rect.size.h <= TITLE_BAR_HEIGHT);
        }
    }
}
