// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/270)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_formats::{form_data, form_template};
use std::{error::Error, fs, io::Write, path::Path};

fn read_structured(path: &std::ffi::OsStr) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(form_data::parse_structured(&fs::read_to_string(path)?)?)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    fs::OpenOptions::new().write(true).create_new(true).open(path)?.write_all(bytes)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [command, template] if command == "inspect" => {
            let fields = form_template::inspect(&fs::read(template)?)?;
            println!("{}", serde_json::to_string_pretty(&fields)?);
        }
        // FORM and RECORD are JSON or YAML.
        [command, template, definition, record, output] if command == "fill" => {
            let result = form_template::fill(
                &fs::read(template)?,
                &read_structured(definition)?,
                &read_structured(record)?,
            )?;
            write_new(Path::new(output), &result)?;
        }
        // One DOCX per CSV row, named record-1.docx, record-2.docx ... in OUTDIR.
        [command, template, definition, csv, out_dir] if command == "fill-csv" => {
            let results = form_template::fill_csv(&fs::read(template)?, &fs::read_to_string(definition)?, &fs::read_to_string(csv)?)?;
            for (n, bytes) in results.iter().enumerate() {
                write_new(&Path::new(out_dir).join(format!("record-{}.docx", n + 1)), bytes)?;
            }
        }
        // The header row of the CSV for this form (a blank sheet to fill in).
        [command, definition] if command == "csv-header" => {
            print!("{}", form_data::records_to_csv(&read_structured(definition)?, &[])?);
        }
        _ => return Err("usage: docx_form inspect TEMPLATE | fill TEMPLATE FORM RECORD OUTPUT.docx | fill-csv TEMPLATE FORM RECORDS.csv OUTDIR | csv-header FORM (FORM and RECORD are JSON or YAML)".into()),
    }
    Ok(())
}
