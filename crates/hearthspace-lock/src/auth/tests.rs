use std::ffi::CStr;

use pam_client::{ConversationHandler, ErrorCode};
use zeroize::Zeroizing;

use super::*;

// The stub replaces only libpam. The production decision and conversation run
// unchanged; ordinary cargo tests never touch /etc/pam.d or authenticate a user.
struct Pam {
    auth: Result<(), Failure>,
    account: Result<(), Failure>,
    user: &'static str,
    account_user: Option<&'static str>,
    accepted: bool,
    account_called: bool,
}

impl Default for Pam {
    fn default() -> Self {
        Self {
            auth: Ok(()),
            account: Ok(()),
            user: "session-user",
            account_user: None,
            accepted: true,
            account_called: false,
        }
    }
}

impl Transaction for Pam {
    fn authenticate(&mut self) -> Result<(), Failure> {
        self.auth
    }
    fn account(&mut self) -> Result<(), Failure> {
        self.account_called = true;
        if let Some(user) = self.account_user {
            self.user = user;
        }
        self.account
    }
    fn user(&self) -> Result<String, Failure> {
        Ok(self.user.into())
    }
    fn conversation_accepted(&self) -> bool {
        self.accepted
    }
}

#[test]
fn successful_password_also_requires_account_policy() {
    let mut pam = Pam {
        account: Err(Failure::Denied),
        ..Pam::default()
    };
    assert_eq!(
        check_transaction(&mut pam, "session-user"),
        Err(Failure::Denied)
    );
    assert!(pam.account_called);
}

#[test]
fn only_successful_authentication_and_account_policy_succeed() {
    let mut pam = Pam::default();
    assert_eq!(check_transaction(&mut pam, "session-user"), Ok(()));
    assert!(pam.account_called);
}

#[test]
fn rejected_password_does_not_run_account_policy() {
    let mut pam = Pam {
        auth: Err(Failure::Denied),
        ..Pam::default()
    };
    assert_eq!(
        check_transaction(&mut pam, "session-user"),
        Err(Failure::Denied)
    );
    assert!(!pam.account_called);
}

#[test]
fn pam_cannot_substitute_the_authenticated_user() {
    let mut pam = Pam {
        user: "different-user",
        ..Pam::default()
    };
    assert_eq!(
        check_transaction(&mut pam, "session-user"),
        Err(Failure::Denied)
    );
    assert!(!pam.account_called);
}

#[test]
fn account_modules_cannot_substitute_the_user_either() {
    let mut pam = Pam {
        account_user: Some("different-user"),
        ..Pam::default()
    };
    assert_eq!(
        check_transaction(&mut pam, "session-user"),
        Err(Failure::Denied)
    );
}

#[test]
fn an_ignored_conversation_error_still_denies_unlock() {
    let mut pam = Pam {
        accepted: false,
        ..Pam::default()
    };
    assert_eq!(
        check_transaction(&mut pam, "session-user"),
        Err(Failure::Denied)
    );
}

fn conversation() -> Conversation {
    Conversation::new(Zeroizing::new("synthetic test credential".into())).unwrap()
}

fn password_prompt() -> &'static CStr {
    c"Password: "
}

#[test]
fn one_secret_prompt_is_answered_without_echo() {
    let mut conversation = conversation();
    assert!(!conversation.accepted());
    let response = conversation.prompt_echo_off(password_prompt()).unwrap();
    assert_eq!(response.to_bytes(), b"synthetic test credential");
    assert!(conversation.accepted());
}

#[test]
fn second_secret_prompt_fails_instead_of_reusing_password_for_otp() {
    let mut conversation = conversation();
    let _ = conversation.prompt_echo_off(password_prompt()).unwrap();
    assert_eq!(
        conversation.prompt_echo_off(c"One-time code:"),
        Err(ErrorCode::CONV_ERR)
    );
    assert!(!conversation.accepted());
}

#[test]
fn echo_prompt_cannot_select_another_account_or_disclose_password() {
    let mut conversation = conversation();
    assert_eq!(
        conversation.prompt_echo_on(c"User:"),
        Err(ErrorCode::CONV_ERR)
    );
    assert_eq!(
        conversation.prompt_echo_off(password_prompt()),
        Err(ErrorCode::CONV_ERR)
    );
    assert!(!conversation.accepted());
}

#[test]
fn error_messages_are_sticky_even_if_a_module_returns_success() {
    let mut conversation = conversation();
    let _ = conversation.prompt_echo_off(password_prompt()).unwrap();
    conversation.error_msg(c"Synthetic error");
    assert!(!conversation.accepted());
}

#[test]
fn binary_and_radio_prompts_fail_closed() {
    let mut binary = conversation();
    assert_eq!(binary.binary_prompt(0, &[]), Err(ErrorCode::CONV_ERR));
    let mut radio = conversation();
    assert_eq!(
        radio.radio_prompt(c"Switch user?"),
        Err(ErrorCode::CONV_ERR)
    );
}

#[test]
fn empty_and_nul_containing_secrets_are_rejected_without_pam() {
    assert!(Conversation::new(Zeroizing::new(String::new())).is_err());
    assert!(Conversation::new(Zeroizing::new("prefix\0suffix".into())).is_err());
}

// Real PAM fixture guidance (opt-in integration, never change host policy):
// In a private mount namespace with mount propagation private, bind a temporary
// directory over /etc/pam.d. Populate only `hearthspace-lock` with a synthetic PAM
// module built in that directory. The module should ask one ECHO_OFF prompt,
// verify a synthetic password, and return separate auth/account status codes.
// Exercise correct/wrong passwords, account expiry/NEW_AUTHTOK_REQD, a changed
// PAM_USER, multiple prompts, a delayed response and worker termination. Also
// remove the fixture service while leaving an `other` pam_permit policy and
// verify rejection. Do not provide a service override or fixture in production.
