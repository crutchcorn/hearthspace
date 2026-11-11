//! Metadata for the frame still owned by the DRM compositor's current KMS state.
//! Retaining a locked image is visual continuity, never new presentation evidence.
#[derive(Default)]
pub(super) enum PresentedScene {
    #[default]
    None,
    Normal(u64),
    Locked(u64),
}

impl PresentedScene {
    pub fn completed(&mut self, sequence: u64, locked: bool) {
        *self = if locked {
            Self::Locked(sequence)
        } else {
            Self::Normal(sequence)
        };
    }

    pub fn security_changed(&mut self, locked: bool) {
        if !(locked && matches!(self, Self::Locked(_))) {
            *self = Self::None;
        }
    }

    pub fn waiting_for_scene(&self, available: bool) -> bool {
        !available && matches!(self, Self::Locked(_))
    }

    pub fn differs_from(&self, sequence: u64) -> bool {
        match self {
            Self::None => true,
            Self::Normal(previous) | Self::Locked(previous) => *previous != sequence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_lock_survives_wake_until_fresh_scene_is_available() {
        let mut presented = PresentedScene::default();
        presented.completed(7, true);
        presented.security_changed(true);
        assert!(
            presented.waiting_for_scene(false),
            "do not insert a black frame"
        );
        assert!(
            !presented.waiting_for_scene(true),
            "fresh locker scene must render"
        );
        assert!(presented.differs_from(8));
        presented.completed(8, true);
        assert!(!presented.differs_from(8));
    }

    #[test]
    fn normal_frames_never_delay_the_initial_opaque_lock_shield() {
        let mut presented = PresentedScene::default();
        presented.completed(7, false);
        presented.security_changed(true);
        assert!(!presented.waiting_for_scene(false));
        assert!(
            presented.differs_from(7),
            "normal frame is not lock evidence"
        );
    }

    #[test]
    fn unlock_or_output_recreation_cannot_reuse_a_previous_lock() {
        let mut presented = PresentedScene::default();
        presented.completed(7, true);
        presented.security_changed(false);
        presented.security_changed(true);
        assert!(!presented.waiting_for_scene(false));
        assert!(presented.differs_from(7));
        let recreated = PresentedScene::default();
        assert!(!recreated.waiting_for_scene(false));
    }
}
