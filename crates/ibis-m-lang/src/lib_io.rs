// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The reading functions: `File.Contents`, `Csv.Document`, `Excel.Workbook`. The reading itself is the host's (`host.rs`).
use crate::builtin::{as_number, as_text};
use crate::error::MError;
use crate::host::CsvOptions;
use crate::table::reg;
use crate::value::*;
use std::collections::HashMap;
use std::rc::Rc;

fn path_of(v: &Value) -> Result<Rc<str>, MError> {
    match v {
        Value::Binary(p) => Ok(p.clone()),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Binary.", v.type_name()))),
    }
}

pub fn register(l: &mut HashMap<&'static str, Value>) {
    for (n, v) in [("TextEncoding.Utf8", 65001.0), ("TextEncoding.Utf16", 1200.0), ("TextEncoding.Ascii", 20127.0), ("TextEncoding.Windows", 1252.0), ("TextEncoding.BigEndianUnicode", 1201.0), ("TextEncoding.Unicode", 1200.0)] {
        l.insert(n, Value::Number(v));
    }
    reg(l, "File.Contents", 1, 2, |_, a| Ok(Value::Binary(as_text(&a[0])?)));
    // `Csv.Document(File.Contents(path), [Delimiter=",", Columns=7, Encoding=65001, QuoteStyle=QuoteStyle.None])`; the second argument may be the
    // number of columns or null.
    reg(l, "Csv.Document", 1, 4, |it, a| {
        let path = path_of(&a[0])?;
        let mut o = CsvOptions { delimiter: ",".to_string(), columns: 0, encoding: 65001, quoted: true };
        match a.get(1) {
            Some(r @ Value::Record(_)) => {
                if let Value::Text(d) = it.field(r, "Delimiter", true)? {
                    o.delimiter = d.to_string();
                }
                if let Value::Number(n) = it.field(r, "Columns", true)? {
                    o.columns = n as usize;
                }
                if let Value::Number(n) = it.field(r, "Encoding", true)? {
                    o.encoding = n as u32;
                }
                if let Value::Number(n) = it.field(r, "QuoteStyle", true)? {
                    o.quoted = n != 0.0;
                }
            }
            Some(Value::Number(n)) => o.columns = *n as usize,
            _ => {}
        }
        let host = it.host()?;
        host.read_csv(&path, &o).map_err(|e| MError::format(e))
    });
    // The sheets and tables of a workbook file. With `useHeaders` true, the first row of each sheet is the names of its columns.
    reg(l, "Excel.Workbook", 1, 3, |it, a| {
        let path = path_of(&a[0])?;
        let mut v = it.host()?.read_workbook(&path).map_err(|e| MError::format(e))?;
        if matches!(a.get(1), Some(Value::Logical(true))) {
            let promote = it.lib.get("Table.PromoteHeaders").cloned().ok_or_else(|| MError::expr("Table.PromoteHeaders is not there."))?;
            if let Value::Table(t) = &v {
                let (di, ki) = (t.columns.iter().position(|c| c == "Data"), t.columns.iter().position(|c| c == "Kind"));
                if let (Some(di), Some(ki)) = (di, ki) {
                    let mut rows = t.rows.clone();
                    for r in &mut rows {
                        if matches!(&r[ki], Value::Text(k) if &**k == "Sheet") {
                            r[di] = it.call(&promote, vec![r[di].clone()])?;
                        }
                    }
                    v = Value::Table(Rc::new(Table { columns: t.columns.clone(), types: t.types.clone(), rows }));
                }
            }
        }
        let _ = as_number;
        Ok(v)
    });
}
