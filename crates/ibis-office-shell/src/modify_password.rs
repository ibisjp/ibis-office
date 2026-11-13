// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 書き込みパスワード (the password to modify: `p:modifyVerifier` of a presentation, `fileSharing` of a workbook,
//! `w:writeProtection` of a document): a file with one is read-only until the password is given (a window asks for it
//! when the file is opened, with 読み取り専用 to go on without it; an information bar asks again later). It is saved
//! under another name, not over the file. ファイル >「書き込みパスワード…」 of a presentation sets or takes it off.
//! Only the hash of the password is in the file (`ibis_ooxml::modify_password`; the old 16-bit hash of `reservationPassword`
//! is `crate::legacy_password::legacy_hash`).
use gpui_kit::*;
use ibis_doc_model::{Command, ModifyVerifier};
use ibis_pptx::ibis_ooxml::modify_password::{hash as password_hash, Algorithm};

use crate::editor::SlideEditor;
use crate::i18n;

/// The rounds a new password is hashed with (what PowerPoint uses).
const SPIN_COUNT: u32 = 100_000;

/// Whether `password` is the one `verifier` was made from.
pub(crate) fn verifier_accepts(v: &ModifyVerifier, password: &str) -> Result<bool, String> {
    if v.algorithm_sid == ModifyVerifier::LEGACY_SID {
        return Ok(v.hash.eq_ignore_ascii_case(&crate::legacy_password::legacy_hash(password)));
    }
    let algorithm = Algorithm::from_sid(v.algorithm_sid).ok_or_else(|| i18n::text("pptx.modify-password.password-modify-uses-method-not").to_string())?;
    let salt = ibis_pptx::ibis_ooxml::base64::decode(&v.salt).ok_or_else(|| i18n::text("pptx.modify-password.password-information-file-damaged").to_string())?;
    let expected = ibis_pptx::ibis_ooxml::base64::decode(&v.hash).ok_or_else(|| i18n::text("pptx.modify-password.password-information-file-damaged").to_string())?;
    Ok(password_hash(algorithm, &salt, password, v.spin_count) == expected)
}

impl SlideEditor {
    /// A file with a password to modify it, not yet given.
    pub fn is_modify_locked(&self) -> bool {
        self.presentation().modify_verifier.is_some() && !self.modify_unlocked
    }

    /// Edits are refused: by the password to modify, or the read-only recommendation not yet set aside.
    pub fn is_pptx_read_only_blocked(&self) -> bool {
        self.is_read_only() || self.is_modify_locked() || (self.is_pptx_read_only_recommended() && !self.is_pptx_read_only_dismissed())
    }

    /// Whether `password` is the one to modify the file; if so, the file can be edited.
    pub fn unlock_modify(&mut self, password: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(v) = self.presentation().modify_verifier.clone() else { return Ok(()) };
        if !verifier_accepts(&v, password)? {
            return Err(i18n::text("_shell.the-password-is-not-correct").to_string());
        }
        self.modify_unlocked = true;
        self.set_status(i18n::text("pptx.modify-password.password-was-accepted-you-can").to_string(), cx);
        cx.notify();
        Ok(())
    }

    /// The password to modify set (`Some`) or taken off (`None`); the file is then edited as before.
    pub fn set_modify_password(&mut self, password: Option<String>, cx: &mut Context<Self>) -> Result<(), String> {
        let verifier = match password {
            Some(password) => Some(new_verifier(&password)?),
            None => None,
        };
        let on = verifier.is_some();
        self.modify_unlocked = true;
        self.run(vec![Command::SetModifyVerifier { verifier }], cx);
        self.set_status(i18n::text(if on { "pptx.modify-password.password-modify-set-goes-into" } else { "pptx.modify-password.password-modify-removed" }).to_string(), cx);
        Ok(())
    }

    /// ファイル > 書き込みパスワード…: the window for it.
    #[cfg(not(target_family = "wasm"))]
    pub fn ask_modify_password_to_set(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_modify_locked() {
            return self.ask_modify_password_to_unlock(window, cx);
        }
        let had = self.presentation().modify_verifier.is_some();
        crate::password_window::ask_to_set_modify(cx.entity().downgrade(), had, window, cx);
    }

    #[cfg(target_family = "wasm")]
    pub fn ask_modify_password_to_set(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_status(i18n::text("_shell.not-available-in-the-browser-version-yet").to_string(), cx);
    }

    /// The password to modify asked for (when the file is opened, or from the information bar).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn ask_modify_password_to_unlock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::password_window::ask_to_modify(cx.entity().downgrade(), window, cx);
    }

    #[cfg(target_family = "wasm")]
    pub(crate) fn ask_modify_password_to_unlock(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_status(i18n::text("_shell.not-available-in-the-browser-version-yet").to_string(), cx);
    }
}

/// A verifier for `password`, with a salt of its own.
#[cfg(not(target_family = "wasm"))]
fn new_verifier(password: &str) -> Result<ModifyVerifier, String> {
    // The OS's random generator (getrandom: Windows has no /dev/urandom, so setting a password to modify failed there; #M150).
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|_| i18n::text("_shell.passwords-cannot-be-set-in-this-environment").to_string())?;
    Ok(ModifyVerifier { algorithm_sid: Algorithm::Sha512.sid(), spin_count: SPIN_COUNT, salt: ibis_pptx::ibis_ooxml::base64::encode(&salt), hash: ibis_pptx::ibis_ooxml::base64::encode(&password_hash(Algorithm::Sha512, &salt, password, SPIN_COUNT)) })
}

#[cfg(target_family = "wasm")]
fn new_verifier(_: &str) -> Result<ModifyVerifier, String> {
    Err(i18n::text("_shell.not-available-in-the-browser-version-yet").to_string())
}

#[cfg(all(test, not(target_family = "wasm")))]
mod new_verifier_tests {
    /// A new password to modify takes its salt from the OS on every OS (Windows has no /dev/urandom: #M150), and the
    /// verifier accepts that password only.
    #[test]
    fn a_new_verifier_accepts_its_password() {
        let v = super::new_verifier("ibis123").expect("a salt from the OS");
        assert!(super::verifier_accepts(&v, "ibis123").unwrap());
        assert!(!super::verifier_accepts(&v, "wrong").unwrap());
        let w = super::new_verifier("ibis123").unwrap();
        assert_ne!(v.salt, w.salt, "each verifier has a salt of its own");
    }
}
