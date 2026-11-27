// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The arguments of the server (design 4.2): `[--allow-dir <folder>]… [--read-only] [--max-documents N] [--max-file-size MB] [--http <port> [--allow-origin <origin>]…]`.

use std::path::PathBuf;

use ibis_doc_service::{Limits, Service};
use serde_json::{Value, json};

const HELP: &str = "ibisOffice --mcp [--allow-dir <folder>]... [--read-only] [--max-documents N] [--max-file-size MB] [--http <port> [--allow-origin <origin>]...]\n\
ibisOffice --mcp [--allow-dir <folder>]... [--read-only] --call <tool> [--args <JSON>] [--folder <folder>] [--revision working-tree|git|open-document] [--commit <sha>] [--expected-revision <value>]\n\n\
Runs a Model Context Protocol server on standard input and output (or, with --http, on 127.0.0.1): docx, xlsx and pptx files under the allowed folders can be read, edited and saved by an AI.\n\
  --allow-dir <folder>   a folder whose files may be opened and saved (repeatable; with none, no file is reachable)\n\
  --read-only            offer only the tools that read\n\
  --max-documents N      documents open at once (default 8)\n\
  --max-file-size MB     the largest file that is opened (default 200)\n\
  --http <port>          listen on http://127.0.0.1:<port>/mcp (Streamable HTTP, no TLS and no sign-in: for OpenAI's Secure MCP Tunnel, whose tunnel-client calls it from this computer) instead of standard input and output; there is no default port\n\
  --allow-origin <origin>  with --http: a web page of this origin may call (repeatable; pages of 127.0.0.1 and localhost always may, calls with no Origin always may)\n\
  --call <tool>          call one tool once and print its result as JSON on standard output (an error: a non-zero exit code and `code: message` on standard error); nothing stays open after it, so a document_id does not reach a next run\n\
  --args <JSON>          with --call: the tool's arguments, a JSON object (default {})\n\
  --folder <folder>      with --call: the argument `folder`, the allowed folder a relative path is from\n\
  --revision <kind>      with --call: the argument `revision.kind` (working-tree, git or open-document: the MCP values with - for _)\n\
  --commit <sha>         with --call: the argument `revision.commit`, the SHA of a Git commit (hexadecimal; not a branch or tag name)\n\
  --expected-revision <value>  with --call: the argument `expected_revision`, the state (or, for a save, the file_sha256) the change is made on; it is refused when the document is not at it\n";

/// What the arguments say, or why they are wrong.
pub(crate) struct Options {
    pub dirs: Vec<PathBuf>,
    pub read_only: bool,
    pub limits: Limits,
    /// The port of `--http`.
    pub http: Option<u16>,
    pub allowed_origins: Vec<String>,
    /// `--call`: the tool, with the arguments `--args` and the flags make.
    pub call: Option<(String, Value)>,
}

pub(crate) fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Options>, String> {
    let mut dirs = Vec::new();
    let mut read_only = false;
    let mut limits = Limits::default();
    let mut http = None;
    let mut allowed_origins = Vec::new();
    let (mut call, mut call_args, mut folder, mut revision, mut commit, mut expected) = (None, None, None, None, None, None);
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let mut value = || iter.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--allow-dir" => dirs.push(PathBuf::from(value()?)),
            "--read-only" => read_only = true,
            "--max-documents" => limits.max_documents = value()?.parse().ok().filter(|n| *n >= 1).ok_or("--max-documents needs a number, 1 or more")?,
            "--max-file-size" => limits.max_file_size = value()?.parse::<u64>().ok().filter(|n| *n >= 1).ok_or("--max-file-size needs a number of megabytes, 1 or more")? * 1024 * 1024,
            "--http" => http = Some(value()?.parse::<u16>().ok().filter(|p| *p >= 1).ok_or("--http needs a port number, 1 to 65535")?),
            "--allow-origin" => allowed_origins.push(value()?),
            "--call" => call = Some(value()?),
            "--args" => call_args = Some(value()?),
            "--folder" => folder = Some(value()?),
            "--revision" => revision = Some(value()?),
            "--commit" => commit = Some(value()?),
            "--expected-revision" => expected = Some(value()?),
            "--help" | "-h" => return Ok(None),
            other => return Err(format!("{other} is not an argument (see --help)")),
        }
    }
    if http.is_none() && !allowed_origins.is_empty() {
        return Err("--allow-origin is for --http".into());
    }
    let call = match call {
        None => {
            if call_args.is_some() || folder.is_some() || revision.is_some() || commit.is_some() || expected.is_some() {
                return Err("--args, --folder, --revision, --commit and --expected-revision are for --call".into());
            }
            None
        }
        Some(tool) => {
            if http.is_some() {
                return Err("--call and --http do not go together".into());
            }
            let mut args: Value = match call_args {
                None => json!({}),
                Some(text) => serde_json::from_str(&text).map_err(|e| format!("--args is not JSON: {e}"))?,
            };
            let map = args.as_object_mut().ok_or("--args is a JSON object")?;
            if let Some(folder) = folder {
                map.insert("folder".into(), json!(folder));
            }
            if revision.is_some() || commit.is_some() {
                // The flags take the place of the same names in --args; with only --commit the version is a commit.
                let mut object = map.get("revision").and_then(Value::as_object).cloned().unwrap_or_default();
                if let Some(kind) = revision {
                    object.insert("kind".into(), json!(kind.replace('-', "_")));
                }
                if let Some(commit) = commit {
                    object.entry("kind").or_insert(json!("git"));
                    object.insert("commit".into(), json!(commit));
                }
                map.insert("revision".into(), Value::Object(object));
            }
            if let Some(expected) = expected {
                map.insert("expected_revision".into(), json!(expected));
            }
            Some((tool, args))
        }
    };
    Ok(Some(Options { dirs, read_only, limits, http, allowed_origins, call }))
}

/// The `ibisOffice` program that draws the exports: this program itself when it is `ibisOffice --mcp`, the `ibisOffice` next to it when it is `ibisOffice-mcp`.
fn exporter() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let stem = me.file_stem()?.to_string_lossy().to_ascii_lowercase();
    if stem == "ibisoffice" {
        return Some(me);
    }
    let sibling = me.with_file_name(format!("ibisOffice{}", std::env::consts::EXE_SUFFIX));
    sibling.is_file().then_some(sibling)
}

/// Run the server on the standard input and output; the exit code.
pub fn run_from_args(args: impl IntoIterator<Item = String>) -> i32 {
    match parse(args) {
        Err(message) => {
            eprintln!("ibisOffice --mcp: {message}");
            2
        }
        Ok(None) => {
            print!("{HELP}");
            0
        }
        Ok(Some(Options { dirs, read_only, limits, http, allowed_origins, call })) => {
            for dir in &dirs {
                if !dir.is_dir() {
                    eprintln!("ibisOffice --mcp: {} is not a folder", dir.display());
                    return 2;
                }
            }
            let mut service = Service::new(&dirs, read_only, limits);
            service.set_exporter(exporter());
            service.set_version(crate::VERSION);
            if let Some((tool, args)) = call {
                return match service.call(&tool, &args) {
                    Ok(result) => {
                        println!("{result}");
                        0
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        1
                    }
                };
            }
            if let Some(port) = http {
                let config = crate::HttpConfig { port, allowed_origins, ..crate::HttpConfig::default() };
                return match crate::HttpServer::bind(service, config) {
                    Ok(server) => {
                        eprintln!("ibisOffice --mcp: listening on http://127.0.0.1:{}/mcp (Streamable HTTP, no sign-in; only for OpenAI's Secure MCP Tunnel)", server.port());
                        server.run();
                        0
                    }
                    Err(e) => {
                        eprintln!("ibisOffice --mcp: cannot listen on 127.0.0.1:{port}: {e}");
                        1
                    }
                };
            }
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            match crate::stdio::serve(service, stdin.lock(), stdout.lock()) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("ibisOffice --mcp: {e}");
                    1
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_arguments_are_read_or_refused() {
        let o = parse(args(&["--allow-dir", "/a", "--allow-dir", "/b", "--read-only", "--max-documents", "3", "--max-file-size", "10"])).unwrap().unwrap();
        assert_eq!((o.dirs.len(), o.read_only, o.limits.max_documents, o.limits.max_file_size, o.http), (2, true, 3, 10 * 1024 * 1024, None));
        let o = parse(args(&[])).unwrap().unwrap();
        assert_eq!((o.dirs.len(), o.read_only, o.limits.max_file_size), (0, false, 200 * 1024 * 1024));
        let o = parse(args(&["--http", "8765", "--allow-origin", "https://a.example"])).unwrap().unwrap();
        assert_eq!((o.http, o.allowed_origins), (Some(8765), vec!["https://a.example".to_string()]));
        assert!(parse(args(&["--help"])).unwrap().is_none());
        for bad in [&["--nope"][..], &["--http"], &["--http", "0"], &["--http", "x"], &["--http", "70000"], &["--allow-origin", "https://a.example"], &["--allow-dir"], &["--max-documents", "0"], &["--max-file-size", "x"]] {
            assert!(parse(args(bad)).is_err(), "{bad:?}");
        }
    }
}
