use nix::unistd::{User, getuid};

/// Resolved once from the real UID, never USER/LOGNAME, arguments or PAM prompts.
#[derive(Clone)]
pub(crate) struct Identity {
    uid: nix::unistd::Uid,
    username: String,
}

impl Identity {
    pub(crate) fn current() -> Result<Self, super::Failure> {
        let uid = getuid();
        let user = User::from_uid(uid)
            .map_err(|_| super::Failure::Unavailable)?
            .ok_or(super::Failure::Unavailable)?;
        if user.uid != uid || user.name.is_empty() || user.name.contains('\0') {
            return Err(super::Failure::Unavailable);
        }
        let identity = Self {
            uid,
            username: user.name,
        };
        if !identity.is_current() {
            return Err(super::Failure::Unavailable);
        }
        Ok(identity)
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(super) fn is_current(&self) -> bool {
        getuid() == self.uid
            && matches!(User::from_uid(self.uid), Ok(Some(user))
                if user.uid == self.uid && user.name == self.username)
            && matches!(User::from_name(&self.username), Ok(Some(user))
                if user.uid == self.uid && user.name == self.username)
    }
}
