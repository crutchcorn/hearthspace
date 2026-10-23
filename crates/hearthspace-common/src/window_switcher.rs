//! Session identity for the passive, compositor-positioned switcher title.
const CAPTION_ID: &str = "dev.hearthspace.window-switcher-caption.session-";

pub fn window_switcher_caption_app_id(session: u64) -> String {
    format!("{CAPTION_ID}{session}")
}

pub fn window_switcher_caption_from_app_id(app_id: &str) -> Option<u64> {
    app_id
        .strip_prefix(CAPTION_ID)?
        .parse()
        .ok()
        .filter(|id| *id > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_require_an_explicit_nonzero_session() {
        for session in [1, 42, u64::MAX] {
            assert_eq!(
                window_switcher_caption_from_app_id(&window_switcher_caption_app_id(session)),
                Some(session)
            );
        }
        for suffix in ["", "0", "-1", "1.other", "18446744073709551616"] {
            assert_eq!(
                window_switcher_caption_from_app_id(&format!("{CAPTION_ID}{suffix}")),
                None
            );
        }
        assert_eq!(
            window_switcher_caption_from_app_id("dev.hearthspace.window-switcher-caption"),
            None
        );
    }
}
