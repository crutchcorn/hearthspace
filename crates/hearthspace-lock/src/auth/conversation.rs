use std::ffi::{CStr, CString};

use pam_client::{ConversationHandler, ErrorCode};
use zeroize::{Zeroize, Zeroizing};

use super::Failure;

/// One password field supports one secret prompt. Further prompts (OTP, password
/// changes, confirmation, user switching) require a different UI and are denied.
/// No PAM messages or secrets are logged, or interpolated into UI feedback.
pub(super) struct Conversation {
    password: Zeroizing<String>,
    supplied: bool,
    rejected: bool,
}

impl Conversation {
    pub(super) fn new(password: Zeroizing<String>) -> Result<Self, Failure> {
        if password.is_empty() || password.contains('\0') {
            return Err(Failure::Denied);
        }
        Ok(Self {
            password,
            supplied: false,
            rejected: false,
        })
    }

    pub(super) fn accepted(&self) -> bool {
        self.supplied && !self.rejected
    }

    fn reject<T>(&mut self) -> Result<T, ErrorCode> {
        self.rejected = true;
        self.password.zeroize();
        Err(ErrorCode::CONV_ERR)
    }
}

impl ConversationHandler for Conversation {
    fn prompt_echo_on(&mut self, _: &CStr) -> Result<CString, ErrorCode> {
        // PAM_USER is already fixed. Never guess an answer to an arbitrary prompt.
        self.reject()
    }

    fn prompt_echo_off(&mut self, _: &CStr) -> Result<CString, ErrorCode> {
        if self.supplied || self.rejected {
            return self.reject();
        }
        self.supplied = true;
        // CString ownership transfers to pam-client, which builds the libpam
        // response. Those library-owned copies follow their libraries' lifetimes;
        // every application-owned secret is erased here or by Zeroizing on drop.
        let response = CString::new(self.password.as_bytes());
        self.password.zeroize();
        response.map_err(|_| {
            self.rejected = true;
            ErrorCode::CONV_ERR
        })
    }

    fn text_info(&mut self, _: &CStr) {}

    fn error_msg(&mut self, _: &CStr) {
        let _: Result<(), _> = self.reject();
    }

    fn radio_prompt(&mut self, _: &CStr) -> Result<bool, ErrorCode> {
        self.reject()
    }

    fn binary_prompt(&mut self, _: u8, _: &[u8]) -> Result<(u8, Vec<u8>), ErrorCode> {
        self.reject()
    }
}
