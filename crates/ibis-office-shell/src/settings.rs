// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The app's settings (ツール > 設定…): the store itself is `ibis_office_base::settings` (a few `key=value` lines in the user's
//! application data directory; in the browser, its localStorage). What is here is the migration at startup, which needs the
//! version of this build (`IBIS_VERSION`, set by this crate's build.rs).

pub use ibis_office_base::settings::{get, remove, set};

/// Run once at application initialization, before any settings are consumed.
/// Keep the old version until every migration succeeds.
pub fn migrate_on_startup() {
    let previous=get("app.last-started-version");
    let current=env!("IBIS_VERSION");
    migrate(previous.as_deref(),current);
    set("app.last-started-version",current);
}
fn version_parts(value:&str)->[u64;3]{let mut parts=[0;3];for (out,part) in parts.iter_mut().zip(value.trim_start_matches('v').split('.')){*out=part.split(|c:char|!c.is_ascii_digit()).next().unwrap_or("").parse().unwrap_or(0);}parts}
/// Future version migrations belong here. Compare parsed numeric components,
/// never version strings (0.10 is newer than 0.9).
fn migrate(previous:Option<&str>,current:&str){
    let upgrade=previous.is_none_or(|v|version_parts(v)<version_parts(current));
    // The old version an update moved aside is removed at the first start of the new one (the updater cannot while the old
    // program still runs).
    #[cfg(not(target_family="wasm"))]
    if upgrade{crate::update::remove_leftover_backup();}
    #[cfg(target_family="wasm")]
    let _=upgrade;
    // セットアップ (K0001) asks about the downloadable fonts at the first start and at the first start of a new version: forgetting the
    // version that asked makes the startup check (`font_setup_dialog::start`) read the list again. (It sets the version again once
    // the list was read, so a start with no network asks at the next one.)
    if upgrade{remove(crate::font_setup::CHECKED_KEY);}
    // No other setting migrations yet; explicit user preferences are preserved.
}

#[cfg(test)]
mod tests {
    use super::{get, migrate, set};
    use crate::font_setup::CHECKED_KEY;

    /// セットアップ (K0001): the check of the downloadable fonts is asked again at the first start and at an update, not at a start of the
    /// same version (or an older one).
    #[test]
    fn the_font_setup_check_is_forgotten_at_the_first_start_and_at_an_update_only() {
        // The first start: no version was recorded.
        set(CHECKED_KEY, "0.0.1");
        migrate(None, "0.8.8");
        assert_eq!(get(CHECKED_KEY), None, "the first start asks");
        // An update.
        set(CHECKED_KEY, "0.8.7");
        migrate(Some("0.8.7"), "0.8.8");
        assert_eq!(get(CHECKED_KEY), None, "an update asks");
        // The same version again, and an older one (a downgrade): the answer stays.
        set(CHECKED_KEY, "0.8.8");
        migrate(Some("0.8.8"), "0.8.8");
        assert_eq!(get(CHECKED_KEY).as_deref(), Some("0.8.8"), "the same version does not ask again");
        migrate(Some("0.8.9"), "0.8.8");
        assert_eq!(get(CHECKED_KEY).as_deref(), Some("0.8.8"));
        // 0.10 is newer than 0.9 (numbers, not text).
        migrate(Some("0.9.0"), "0.10.0");
        assert_eq!(get(CHECKED_KEY), None);
        crate::settings::remove(CHECKED_KEY);
    }
}
