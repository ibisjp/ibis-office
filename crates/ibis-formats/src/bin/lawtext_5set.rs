// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use ibis_formats::lawtext::{
    LawDoc, ShinkyuOptions, build_5set_package_with_sources, parse_law_input,
};

const USAGE: &str = "Usage: ibis-lawtext-5set OLD NEW [--memo FILE] [--reference FILE]... [--old-on-left] [--json | --docx] [--require-match]\n    Inputs: UTF-8 e-Gov XML, Lawtext or law Markdown. Output: combined Markdown, JSON or print-layout DOCX to stdout.\n    --require-match exits with status 2 after writing drafts if reverse application differs.";

fn read(path: &OsStr) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", Path::new(path).display()))
}

fn read_law(path: &OsStr) -> Result<LawDoc, String> {
    parse_law_input(&read(path)?).map_err(|e| format!("{}: {e}", Path::new(path).display()))
}

fn run() -> Result<ExitCode, String> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_deref() == Some(OsStr::new("--help")) {
        println!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    let (Some(old), Some(new)) = (first, args.next()) else {
        return Err(USAGE.to_owned());
    };

    let mut memo = String::new();
    let mut sources = Vec::new();
    let mut options = ShinkyuOptions::default();
    let mut json = false;
    let mut docx = false;
    let mut require_match = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--memo" | "--reference") => {
                let path = args
                    .next()
                    .ok_or_else(|| format!("Missing file for {}", arg.to_string_lossy()))?;
                if arg == "--memo" {
                    memo = read(&path)?;
                } else {
                    sources.push(read_law(&path)?);
                }
            }
            Some("--old-on-left") => options.new_on_left = false,
            Some("--json") => json = true,
            Some("--docx") => docx = true,
            Some("--require-match") => require_match = true,
            _ => {
                return Err(format!(
                    "Unknown option: {}\n{USAGE}",
                    arg.to_string_lossy()
                ));
            }
        }
    }

    if json && docx {
        return Err("--json and --docx are mutually exclusive".into());
    }
    let old = read_law(&old)?;
    let new = read_law(&new)?;
    if !old.law_num.is_empty() && !new.law_num.is_empty() && old.law_num != new.law_num {
        return Err(format!(
            "法令番号が一致しません（現行 {}、改正後 {}）",
            old.law_num, new.law_num
        ));
    }
    let package = build_5set_package_with_sources(&old, &new, &memo, &options, &sources);
    let mut stdout = io::stdout().lock();
    if docx {
        let bytes = package.to_print_docx().map_err(|e| e.to_string())?;
        stdout.write_all(&bytes).map_err(|e| e.to_string())?;
    } else if json {
        serde_json::to_writer_pretty(&mut stdout, &package.to_json_value())
            .map_err(|e| e.to_string())?;
        writeln!(stdout).map_err(|e| e.to_string())?;
    } else {
        stdout
            .write_all(package.to_markdown().as_bytes())
            .map_err(|e| e.to_string())?;
    }
    stdout.flush().map_err(|e| e.to_string())?;

    if require_match && !package.aratame_verification.is_exact_match {
        eprintln!("改め文の逆適用結果に要確認事項があります。");
        return Ok(ExitCode::from(2));
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
