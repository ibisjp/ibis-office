// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Puts ibisOffice in the Explorer's "Open with" list of the text tables (`.csv`, `.tsv`), and gives the files it opens, once a user makes it
//! their program for them, the icon of their kind (K0048): the blue document, the green spreadsheet and the orange presentation icon (resources
//! 2, 3 and 4 of the executable, `icons/doc/`), not the app's own icon, through one program ID for each kind (`ibisOffice.Document`, ...).
//!
//! The app is delivered as a folder (no installer), so it registers itself, for the user
//! only (`HKCU\Software\Classes`, no administrator needed), each time it starts from a
//! folder it is not yet registered for. It does not take the files over: only the
//! "Open with" list gets the entry, the default program stays as the user set it.

use std::os::windows::process::CommandExt;
use std::process::Command;

const KEY: &str = r"HKCU\Software\Classes\Applications\ibisOffice.exe";
const EXTENSIONS: &[&str] = &[".csv", ".tsv"];
const CLASSES: &str = r"HKCU\Software\Classes";

/// The kinds of file: the program ID, the icon resource of the executable, the name Explorer shows as the type, and the extensions.
const KINDS: &[(&str, u32, &str, &[&str])] = &[
    ("ibisOffice.Document", 2, "ibisOffice document", &[".docx", ".docm", ".dotx", ".dotm", ".doc", ".dot", ".odt", ".ott"]),
    ("ibisOffice.Spreadsheet", 3, "ibisOffice spreadsheet", &[".xlsx", ".xlsm", ".xltx", ".xltm", ".xls", ".xlt", ".ods", ".ots", ".csv", ".tsv"]),
    ("ibisOffice.Presentation", 4, "ibisOffice presentation", &[".pptx", ".pptm", ".ppsx", ".ppsm", ".potx", ".potm", ".ppt", ".pps", ".pot", ".odp", ".otp"]),
];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn reg(args: &[&str]) -> Option<String> {
    let out = Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Registers this executable (in the background; a failure is ignored: the entry is a convenience).
pub fn register() {
    let Ok(exe) = std::env::current_exe() else { return };
    let command = format!("\"{}\" \"%1\"", exe.display());
    std::thread::spawn(move || {
        let command_key = format!(r"{KEY}\shell\open\command");
        // Already registered for this folder (the kinds' icons too).
        let icon_of = |resource: u32| format!("\"{}\",-{resource}", exe.display());
        let first_icon = icon_of(KINDS[0].1);
        if reg(&["query", &command_key, "/ve"]).is_some_and(|out| out.contains(&command))
            && reg(&["query", &format!(r"{CLASSES}\{}\DefaultIcon", KINDS[0].0), "/ve"]).is_some_and(|out| out.contains(&first_icon))
        {
            return;
        }
        reg(&["add", &command_key, "/ve", "/d", &command, "/f"]);
        reg(&["add", KEY, "/v", "FriendlyAppName", "/d", "ibisOffice", "/f"]);
        // The kinds: the type's name, its icon and how it opens; each extension lists the kind among the programs it can open with.
        for (progid, resource, name, extensions) in KINDS {
            let key = format!(r"{CLASSES}\{progid}");
            reg(&["add", &key, "/ve", "/d", *name, "/f"]);
            reg(&["add", &format!(r"{key}\DefaultIcon"), "/ve", "/d", &icon_of(*resource), "/f"]);
            reg(&["add", &format!(r"{key}\shell\open\command"), "/ve", "/d", &command, "/f"]);
            for extension in *extensions {
                reg(&["add", &format!(r"{CLASSES}\{extension}\OpenWithProgids"), "/v", *progid, "/t", "REG_NONE", "/f"]);
            }
        }
        for extension in EXTENSIONS {
            reg(&["add", &format!(r"{KEY}\SupportedTypes"), "/v", extension, "/d", "", "/f"]);
            reg(&["add", &format!(r"HKCU\Software\Classes\{extension}\OpenWithList\ibisOffice.exe"), "/f"]);
        }
    });
}
