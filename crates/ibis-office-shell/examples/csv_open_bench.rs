// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Time to open a CSV (the first chunk of 100,000 rows) without and with the chosen font (#243); the median of five runs each.
//! `cargo run --release -p ibis-office-shell --example csv_open_bench -- target/csv100.csv`
use ibis_formats::csv::CsvFont;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn time(label: &str, mut f: impl FnMut()) {
    let runs: Vec<f64> = (0..5)
        .map(|_| {
            let t = std::time::Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    println!("{label}: median {:.3} ms (runs {:?})", median(runs.clone()), runs.iter().map(|r| (r * 1000.0).round() / 1000.0).collect::<Vec<_>>());
}

fn main() {
    let path = std::env::args().nth(1).expect("a csv path");
    let data: std::sync::Arc<[u8]> = std::fs::read(&path).unwrap().into();
    let font = CsvFont { name: "Arial".into(), size_pt: 14.0 };
    time("csv_font() (the settings read, none chosen)", || {
        std::hint::black_box(ibis_office_shell::settings_dialog::csv_font());
    });
    // The first open warms the caches.
    let _ = ibis_formats::open_progressive(data.clone(), 100_000).unwrap();
    for _ in 0..2 {
        time("open_progressive (as before)", || {
            let _ = ibis_formats::open_progressive(data.clone(), 100_000).unwrap();
        });
        time("open_progressive_with(None) + settings read", || {
            let f = ibis_office_shell::settings_dialog::csv_font();
            let _ = ibis_formats::open_progressive_with(data.clone(), 100_000, f.as_ref()).unwrap();
        });
        time("open_progressive_with(Arial 14)", || {
            let _ = ibis_formats::open_progressive_with(data.clone(), 100_000, Some(&font)).unwrap();
        });
    }
}
