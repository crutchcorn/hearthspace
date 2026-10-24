//! The only authentication path: the real Unix user and a fixed system PAM policy.

mod conversation;
mod identity;
mod worker;

pub(crate) use identity::Identity;
pub(crate) use worker::{Attempt, Completion, Poll};

use std::fs::OpenOptions;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

use nix::unistd::{geteuid, getuid};
use pam_client::{Context, Flag};
use zeroize::Zeroizing;

use conversation::Conversation;

const PAM_SERVICE: &str = "hearthspace-lock";
const PAM_POLICY: &str = "/etc/pam.d/hearthspace-lock";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Failure {
    Denied,
    Unavailable,
}

impl Failure {
    pub(crate) fn feedback(self) -> &'static str {
        match self {
            Self::Denied => "That password didn’t match. Try again.",
            Self::Unavailable => "Authentication is unavailable. The session remains locked.",
        }
    }
}

fn authenticate(identity: &Identity, password: Zeroizing<String>) -> Result<(), Failure> {
    // Check access as the actual authentication user without changing process
    // credentials. Elevated effective credentials must not mask unreadability.
    if !identity.is_current() || geteuid() != getuid() {
        return Err(Failure::Unavailable);
    }
    // PAM can fall back to `other` for an unreadable policy too. Open for reading
    // and check that descriptor, rejecting symlinks and avoiding FIFO blocking.
    let policy = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(PAM_POLICY)
        .map_err(|_| Failure::Unavailable)?;
    let metadata = policy.metadata().map_err(|_| Failure::Unavailable)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(Failure::Unavailable);
    }
    let conversation = Conversation::new(password)?;
    let mut context = Context::new(PAM_SERVICE, Some(identity.username()), conversation)
        .map_err(|_| Failure::Unavailable)?;
    check_transaction(&mut context, identity.username())?;
    // Re-resolve through NSS: no renamed/reassigned account may unlock this session.
    if !identity.is_current() {
        return Err(Failure::Denied);
    }
    Ok(())
}

// This boundary lets tests exercise the complete decision, including PAM account
// policy and user substitution, without authenticating against the host machine.
trait Transaction {
    fn authenticate(&mut self) -> Result<(), Failure>;
    fn account(&mut self) -> Result<(), Failure>;
    fn user(&self) -> Result<String, Failure>;
    fn conversation_accepted(&self) -> bool;
}

impl Transaction for Context<Conversation> {
    fn authenticate(&mut self) -> Result<(), Failure> {
        self.authenticate(Flag::DISALLOW_NULL_AUTHTOK)
            .map_err(|_| Failure::Denied)
    }

    fn account(&mut self) -> Result<(), Failure> {
        self.acct_mgmt(Flag::NONE).map_err(|_| Failure::Denied)
    }

    fn user(&self) -> Result<String, Failure> {
        self.user().map_err(|_| Failure::Denied)
    }

    fn conversation_accepted(&self) -> bool {
        self.conversation().accepted()
    }
}

fn check_transaction(pam: &mut impl Transaction, username: &str) -> Result<(), Failure> {
    pam.authenticate()?;
    if !pam.conversation_accepted() || pam.user()? != username {
        return Err(Failure::Denied);
    }
    // Expired credentials, password-change requests and account restrictions all
    // fail closed. A screen locker must never change passwords or open a session.
    pam.account()?;
    if !pam.conversation_accepted() || pam.user()? != username {
        return Err(Failure::Denied);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod private_policy_tests;
