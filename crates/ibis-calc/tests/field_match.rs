// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use ibis_calc::eval::Book;
use ibis_calc::formula::to_file;
use ibis_calc::recalc::evaluate;
use ibis_calc::{CellError, CellValue};

struct Sheet {
    date1904: bool,
}

impl Book for Sheet {
    fn sheet_count(&self) -> usize {
        1
    }
    fn sheet_name(&self, _: usize) -> &str {
        "Sheet1"
    }
    fn value(&self, _: usize, row: u32, column: u32) -> CellValue {
        match (row, column) {
            (0, 0) => CellValue::Text("東京都千代田区霞が関二丁目１番２号".into()),
            (0, 1) => CellValue::Text("東京都千代田区霞が関2-1-2".into()),
            _ => CellValue::Empty,
        }
    }
    fn formula(&self, _: usize, _: u32, _: u32) -> Option<&str> {
        None
    }
    fn for_each_cell(&self, _: usize, _: (u32, u32, u32, u32), _: &mut dyn FnMut(u32, u32, &CellValue)) {}
    fn extent(&self, _: usize) -> (u32, u32) {
        (1, 2)
    }
    fn defined_name(&self, _: &str, _: Option<usize>) -> Option<String> {
        None
    }
    fn name_variant_cluster(&self, c: char) -> Option<u32> {
        matches!(c, '髙' | '高').then_some(1)
    }
    fn date1904(&self) -> bool {
        self.date1904
    }
}

fn check(args: &str, expected: CellValue) {
    let formula = format!("IBIS.FIELDMATCH({args})");
    let sheet = Sheet { date1904: false };
    assert_eq!(evaluate(&sheet, (0, 2, 0), &formula), (expected.clone(), false), "{formula}");
    let stored = to_file(&formula);
    assert!(stored.starts_with("_xlfn.IBIS.FIELDMATCH("), "{stored}");
    assert_eq!(evaluate(&sheet, (0, 2, 0), &stored), (expected, false), "{stored}");
}

#[test]
fn date_precision_and_era_bounds() {
    for (args, rank) in [
        (r#""令和元年5月1日",DATE(2019,5,1),"date""#, 2.0),
        (r#""昭和60年4月","1985-04-01","date""#, 1.0),
        (r#""2019-05","平成31年","date""#, 0.0),
        (r#""平成元年1月","1989-01-07","date""#, 0.0),
        (r#""平成元年1月","1989-01-08","date""#, 1.0),
        (r#""平成31年","令和元年","date""#, 0.0),
        (r#""2019","平成31年","date""#, 1.0),
        (r#""令和元年4月","2019-04-01","date""#, -1.0),
        (r#""1985-04-01","85/4/1","date""#, 1.0),
        (r#""2023-02-29","2023-02","date""#, -1.0),
        (r#"60,"1900-02","date""#, -1.0),
    ] {
        check(
            args,
            if rank < 0.0 {
                CellValue::Error(CellError::NA)
            } else {
                CellValue::Number(rank)
            },
        );
    }
    assert_eq!(
        evaluate(&Sheet { date1904: true }, (0, 2, 0), r#"IBIS.FIELDMATCH(0,"1904-01-01","date")"#),
        (CellValue::Number(2.0), false)
    );
}

#[test]
fn address_tokens_preserve_location_details() {
    for (args, rank) in [
        (r#"A1,B1,"ADDRESS""#, 2.0),
        (r#""霞が関二丁目十二番二十号","霞が関 2‐12−20","address""#, 2.0),
        (r#""霞が関2-1-2","霞が関2-1-3","address""#, 0.0),
        (r#""霞が関2-1-2 A棟101","霞が関2-1-2 A棟102","address""#, 0.0),
        (r#""霞ヶ関2-1-2","霞が関2-1-2","address""#, 1.0),
        (r#""霞が関1 2丁目1番2号","霞が関12-1-2","address""#, 0.0),
        (r#""霞が関2ー1ー2","霞が関2-1-2","address""#, 0.0),
        (r#""霞が関二十十丁目1番2号","霞が関30-1-2","address""#, 0.0),
    ] {
        check(args, CellValue::Number(rank));
    }
}

#[test]
fn missing_values_and_errors_are_not_matches() {
    check(r#""男性","Ｍ","gender""#, CellValue::Number(2.0));
    check(r#""女性","MALE","gender""#, CellValue::Number(0.0));
    check(r#"1,1,"gender""#, CellValue::Error(CellError::NA));
    check(r#""不明","不明","gender""#, CellValue::Error(CellError::NA));
    check(r#"C1," ","address""#, CellValue::Error(CellError::NA));
    check(r#"1/0,2,"date""#, CellValue::Error(CellError::Div0));
    check(r#"A1,B1,"other""#, CellValue::Error(CellError::Value));
}

#[test]
fn arrays_can_filter_candidates() {
    let result = evaluate(
        &Sheet { date1904: false },
        (0, 2, 0),
        r#"SUM(IBIS.FIELDMATCH({"1985-04-01","1985-04","1985-05"},"1985-04-01","date"))"#,
    );
    assert_eq!(result, (CellValue::Number(3.0), false));
}

#[test]
fn a_broadcast_address_keeps_per_cell_modes_and_errors() {
    let sheet = Sheet { date1904: false };
    for (formula, expected) in [
        (
            r#"SUM(IBIS.FIELDMATCH({"霞が関二丁目1番2号"},{"霞が関2-1-2","霞ヶ関2-1-2","霞が関2-1-3"},{"ADDRESS"}))"#,
            CellValue::Number(3.0),
        ),
        (
            r#"SUM(IBIS.FIELDMATCH("霞が関二丁目1番2号",{"霞が関2-1-2","霞が関2-1-2"},{"address","gender"}))"#,
            CellValue::Error(CellError::NA),
        ),
        (
            r#"INDEX(IBIS.FIELDMATCH(" ",{"霞が関2-1-2",#DIV/0!},"address"),1,2)"#,
            CellValue::Error(CellError::Div0),
        ),
    ] {
        assert_eq!(evaluate(&sheet, (0, 2, 0), formula), (expected, false), "{formula}");
    }
}

#[test]
fn juki_gaiji_marker_requires_explicit_opt_in() {
    for (left, right, mode) in [
        ("山휀花子", "山田花子", "name"),
        ("霞が휀2-1-2", "霞が関2-1-2", "address"),
    ] {
        let args = format!(r#""{left}","{right}","{mode}""#);
        check(&args, CellValue::Number(0.0));
        check(&format!("{args},UNICHAR(55040)"), CellValue::Number(1.0));
        check(&format!(r#""{left}","{left}","{mode}""#), CellValue::Number(2.0));
    }
}

#[test]
fn address_place_variants_and_wildcards_are_possible_matches() {
    for (args, result) in [
        (r#""霞ケ関2-1-2","霞が関2-1-2","address""#, 1.0),
        (r#""○町大字山田字上1","○町山田上1","address""#, 1.0),
        (r#""○町字山田1","○町山田1","address""#, 1.0),
        (r#""東京都八丈島八丈町大賀郷2551-2","東京都八丈町大賀郷2551-2","address""#, 1.0),
        (r#""東京都八丈島三宅村阿古497","東京都三宅村阿古497","address""#, 0.0),
        (r#""霞ヶ●2-1-2","霞が関2-1-2","address""#, 1.0),
        (r#""霞〓関2-1-2","霞が関2-1-2","address""#, 1.0),
        (r#""霞?関2-1-2","霞？関2-1-2","address""#, 1.0),
        (r#""●●●2-1-2","霞が関2-1-2","address""#, 0.0),
        (r#""霞●2-1-2","霞が関2-1-2","address""#, 0.0),
        (r#""霞●関2-1-2","霞が関2-1-3","address""#, 0.0),
        (r#""霞●関2-1-2 A棟101","霞が関2-1-2 A棟102","address""#, 0.0),
        (r#""霞●関二丁目 A棟?","霞が関二丁目 A棟101","address""#, 0.0),
    ] {
        check(args, CellValue::Number(result));
    }
    for substitute in ['\u{001a}', '\u{fffd}', '\u{3013}', '\u{25cf}', '\u{25a0}', '?', '？'] {
        check(&format!(r#""霞{substitute}関2-1-2","霞が関2-1-2","address""#), CellValue::Number(1.0));
    }
    for literal in ['\u{fffc}', '\u{fffe}', '△', '□', '한'] {
        check(&format!(r#""霞{literal}関2-1-2","霞が関2-1-2","address""#), CellValue::Number(0.0));
    }
    let external = '\u{e123}';
    check(
        &format!(r#""霞{external}関2-1-2","霞が関2-1-2","address","{external}""#),
        CellValue::Number(1.0),
    );
    check(&format!(r#""霞{external}関2-1-2","霞が関2-1-2","address""#), CellValue::Number(0.0));
    check(r#""男性","Ｍ","gender","●""#, CellValue::Error(CellError::Value));
}

#[test]
fn names_distinguish_exact_spelling_from_possible_matches() {
    for (args, expected) in [
        (r#""山田　花子","山田花子","name""#, 2.0),
        (r#""髙橋 花子","高橋花子","name""#, 1.0),
        (r#""田中（旧姓：山田）花子","山田花子","name""#, 1.0),
        (r#""田中・旧姓山田 花子","山田花子","name""#, 1.0),
        (r#""田中（旧姓：山田）花子","佐藤 (旧姓 山田) 花子","name""#, 1.0),
        (r#""鈴木 花子","田中 花子","name""#, 1.0),
        (r#""鈴木花子","田中花子","name""#, 0.0),
        (r#""鈴木 花子","田中 花●","name""#, 0.0),
        (r#""田中 ●子","田中 花子","name""#, 1.0),
        (r#""〓〓 花子","田中 花子","name""#, 1.0),
        (r#""〓〓","田中","name""#, 0.0),
        (r#""田中 花子","山田 太郎","name""#, 0.0),
        (r#""田中（旧姓：山田）花子","山田春子","name""#, 0.0),
    ] {
        check(args, CellValue::Number(expected));
    }
    let mark = '\u{e123}';
    check(&format!(r#""田{mark} 花子","田中花子","name","{mark}""#), CellValue::Number(1.0));
}

// Adapted from synthetic fixtures by the soroe contributors.
// https://github.com/digital-go-jp/soroe/tree/a3b750a2efde43b103678d2ecc0f07ba3c2d9f69/tests/cases
// Fixture paths are relative to that directory. The expected ranks follow
// ibisOffice's review policy, including a rank of 1 for explicit former names.
#[test]
fn soroe_scenarios_keep_ambiguous_candidates_distinct() {
    for (case, mode, left, right, rank) in [
        // reconcile-record/mixed_shape_old_name_keyword.yaml
        ("former surname", "name", "田中 (旧姓: 山田) 太郎", "山田 太郎", 1.0),
        ("former surname with different given name", "name", "田中 (旧姓: 山田) 太郎", "山田 一郎", 0.0),
        // name/wildcard_default.yaml, with synthetic counterpart spellings.
        ("geta", "name", "〓田中", "竹田中", 1.0),
        ("black circle", "name", "田●中", "田野中", 1.0),
        ("black square", "name", "田中■", "田中学", 1.0),
        ("replacement character", "name", "田中�", "田中学", 1.0),
        ("wildcard with conflicting known text", "name", "田●中", "山野中", 0.0),
        // reconcile-date/possible_partial.yaml and false_year_mismatch_partial.yaml
        ("year only", "date", "1985年", "1985-04-15", 1.0),
        ("year and month", "date", "1985年4月", "1985-04-15", 1.0),
        ("different month", "date", "1985年4月", "1985-05-15", 0.0),
        ("different year", "date", "1995-08", "1996-08", 0.0),
        // address/island_miyake_hachijo.yaml and wildcard_unique.yaml
        ("island prefix", "address", "東京都三宅島三宅村阿古497", "東京都三宅村阿古497", 1.0),
        (
            "island prefix with different lot",
            "address",
            "東京都三宅島三宅村阿古497",
            "東京都三宅村阿古498",
            0.0,
        ),
        ("ward wildcard", "address", "東京都●●●区霞が関2-1-2", "東京都千代田区霞が関2-1-2", 1.0),
        (
            "each ward wildcard covers exactly one character",
            "address",
            "東京都●●●区銀座5-10-1",
            "東京都中央区銀座5-10-1",
            0.0,
        ),
    ] {
        for (a, b) in [(left, right), (right, left)] {
            let args = format!(r#""{a}","{b}","{mode}""#);
            check(&args, CellValue::Number(rank));

            let review = format!(r#"IF(IFNA(IBIS.FIELDMATCH({args})=1,FALSE),"要確認","")"#);
            let expected = if rank == 1.0 { "要確認" } else { "" };
            assert_eq!(
                evaluate(&Sheet { date1904: false }, (0, 2, 0), &review),
                (CellValue::Text(expected.into()), false),
                "{case}: {review}",
            );
        }
    }
}

#[test]
fn soroe_marriage_scenario_can_be_filtered_by_date_and_address() {
    // reconcile-record/multi_field_marriage.yaml, with exact-name and missing-date controls.
    for (right_name, date, lot, expected) in [
        ("田中 花子", "1990-12-15", 2, "要確認"),
        ("田中 花子", "1990-12-15", 3, ""),
        ("田中 花子", "1990-12-16", 2, ""),
        ("田中 花子", "", 2, ""),
        ("鈴木 花子", "1990-12-15", 2, ""),
        ("佐藤 太郎", "1990-12-15", 2, ""),
    ] {
        let formula = format!(
            concat!(
                r#"IF(IFNA(AND(IBIS.FIELDMATCH("鈴木 花子","{right_name}","name")=1,"#,
                r#"IBIS.FIELDMATCH("1990-12-15","{date}","date")=2,"#,
                r#"IBIS.FIELDMATCH("東京都千代田区霞が関2-1-2","東京都千代田区霞が関2-1-{lot}","address")=2),FALSE),"要確認","")"#,
            ),
            right_name = right_name,
            date = date,
            lot = lot
        );
        assert_eq!(
            evaluate(&Sheet { date1904: false }, (0, 2, 0), &formula),
            (CellValue::Text(expected.into()), false),
            "{formula}",
        );
    }
}

#[test]
fn phone_formats_and_missing_extensions() {
    // The domestic/international pair builds on soroe's basic-jp fixture.
    for (args, rank) in [
        (r#""03-1234-5678","+81312345678","phone""#, 2.0),
        (r#""（０３）１２３４－５６７８","+81 (0)3 1234 5678","phone""#, 2.0),
        (r#""03-1234-5678 内線012","+81-(0)3-1234-5678 EXT.012","phone""#, 2.0),
        (r#""03-1234-5678#123","+81 3 1234 5678","phone""#, 1.0),
        (r#""+81 3 1234 5678","03-1234-5678 x123","phone""#, 1.0),
        (r#""03-1234-5678 内線012","03-1234-5678 内線12","phone""#, 0.0),
        (r#""03-1234-5678","+81 3 1234 5679","phone""#, 0.0),
        (r#""+1 (202) 555-0100","+12025550100","phone""#, 2.0),
    ] {
        check(args, CellValue::Number(rank));
    }
    for args in [
        r#""1234-5678","03-1234-5678","phone""#,
        r#""03-1234-5678 内線","03-1234-5678","phone""#,
        r#""03--1234-5678","03-1234-5678","phone""#,
        r#""03)1234-5678","03-1234-5678","phone""#,
        r#""+81 03 1234 5678","03-1234-5678","phone""#,
        r#"312345678,"03-1234-5678","phone""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
}

#[test]
fn postal_codes_preserve_the_evidence_of_leading_zeros() {
    for (args, rank) in [
        (r#""〒０６０－０００１","0600001","postal""#, 2.0),
        (r#"600001,"060-0001","postal""#, 1.0),
        (r#""001-0001",10001,"postal""#, 1.0),
        (r#""100-0001",1000001,"postal""#, 2.0),
        (r#"600001,"060-0002","postal""#, 0.0),
        (r#""060-0001","060-0002","postal""#, 0.0),
    ] {
        check(args, CellValue::Number(rank));
    }
    for args in [
        r#""600001","060-0001","postal""#,
        r#"600001,600001,"postal""#,
        r#""060","060-0001","postal""#,
        r#""〒060-0001 札幌市","060-0001","postal""#,
        r#"600001.5,"060-0001","postal""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
}

#[test]
fn email_case_folding_is_limited_to_the_domain() {
    for (args, rank) in [
        (r#""  Billing@EXAMPLE.COM　","Billing@example.com","email""#, 2.0),
        (r#""Billing@example.com","billing@example.com","email""#, 0.0),
        (r#""billing+test@example.com","billing@example.com","email""#, 0.0),
        (r#""bill.ing@example.com","billing@example.com","email""#, 0.0),
        (r#""billing@EXAMPLE.ORG","billing@example.com","email""#, 0.0),
    ] {
        check(args, CellValue::Number(rank));
    }
    for args in [
        r#""@example.com","billing@example.com","email""#,
        r#""bill..ing@example.com","billing@example.com","email""#,
        r#""billing@-example.com","billing@example.com","email""#,
        r#""billing@@example.com","billing@example.com","email""#,
        r#""billing@例.example","billing@example.com","email""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
}

#[test]
fn corporate_number_checks_both_inputs_before_comparison() {
    // Public examples from soroe's corporate_number tests.
    check(r#""１１８０－３０１０－１８７７１",1180301018771,"corporate-number""#, CellValue::Number(2.0));
    check(r#"1180301018771,7010001065919,"corporate-number""#, CellValue::Number(0.0));
    for args in [
        r#"5180301018771,5180301018771,"corporate-number""#,
        r#"1180301018771,5180301018771,"corporate-number""#,
        r#"5180301018771,1180301018771,"corporate-number""#,
        r#"1180301018771.5,1180301018771,"corporate-number""#,
        r#""118030101877?","1180301018771","corporate-number""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
}

#[test]
fn mynumber_preserves_leading_zeros_and_verifies_the_check_digit() {
    // Synthetic bodies exercise both remainder branches of the mod-11 rule.
    for (args, rank) in [
        (r#""０１２３－４５６７－８９０３","012345678903","mynumber""#, 2.0),
        (r#""123456789018",123456789018,"mynumber""#, 2.0),
        (r#""0000 0000 0060","000000000060","mynumber""#, 2.0),
        (r#""000000000000","000000000060","mynumber""#, 0.0),
        (r#""123456789018","234567890121","mynumber""#, 0.0),
    ] {
        check(args, CellValue::Number(rank));
    }
    for args in [
        r#""012345678903",12345678903,"mynumber""#,
        r#""123456789019","123456789019","mynumber""#,
        r#""123456789019","123456789018","mynumber""#,
        r#""123456789018","123456789019","mynumber""#,
        r#""000000000061","000000000060","mynumber""#,
        r#""12345678901?","123456789018","mynumber""#,
        r#"123456789018.5,"123456789018","mynumber""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
}

#[test]
fn drivers_license_checks_position_eleven_and_flags_reissue() {
    for (args, rank) in [
        (r#""１２３４－５６７８－９０３２",123456789032,"drivers-license""#, 2.0),
        (r#""123456789032","123456789033","drivers-license""#, 1.0),
        (r#""123456789033","123456789032","drivers-license""#, 1.0),
        (r#""123456789032","309212345690","drivers-license""#, 0.0),
        (r#""300000000400","300000000401","drivers-license""#, 1.0),
        (r#""300000000910","300000000911","drivers-license""#, 1.0),
    ] {
        check(args, CellValue::Number(rank));
    }
    for args in [
        r#""123456789042","123456789032","drivers-license""#,
        r#""123456789042","123456789042","drivers-license""#,
        r#""12345678903?","123456789032","drivers-license""#,
        r#""012345678920",12345678920,"drivers-license""#,
        r#""300000000900","300000000900","drivers-license""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
}

#[test]
fn japanese_passport_numbers_compare_only_complete_printed_numbers() {
    check(r#"" ｔｋ－０１２３４５６ ","TK0123456","passport-jp""#, CellValue::Number(2.0));
    check(r#""TK0123456","TM0123456","passport-jp""#, CellValue::Number(0.0));
    check(r#""TK0123456","TK0123457","passport-jp""#, CellValue::Number(0.0));
    for args in [
        r#""T0123456","TK0123456","passport-jp""#,
        r#""TK123456","TK0123456","passport-jp""#,
        r#""TK0123456<","TK0123456","passport-jp""#,
        r#""TKO123456","TK0123456","passport-jp""#,
        r#""ＴＫ０１２３４５？","TK0123456","passport-jp""#,
        r#"123456,"TK0123456","passport-jp""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
}

#[test]
fn japanese_passport_mrz_verifies_the_issuer_and_check_digits() {
    // Synthetic, two-line ICAO TD3 example.
    const UPPER: &str = "P<JPNTEST<<SAMPLE<<<<<<<<<<<<<<<<<<<<<<<<<<<";
    const LOWER: &str = "TK01234568JPN9001011<3012316<<<<<<<<<<<<<<02";
    const DIFFERENT_NUMBER: &str = "TK01234579JPN9001011<3012316<<<<<<<<<<<<<<00";
    let mrz = format!("{UPPER}\n{LOWER}");
    let crlf = format!("{UPPER}\r\n{LOWER}");
    let mismatch = format!("{UPPER}\n{DIFFERENT_NUMBER}");
    let different_nationality = mrz.replacen("68JPN", "68USA", 1);
    let unused_optional = format!("{UPPER}\nTK01234568JPN9001011<3012316<<<<<<<<<<<<<<<2");
    let used_optional = format!("{UPPER}\nTK01234568JPN9001011<3012316A1<<<<<<<<<<<<38");
    for (left, right, rank) in [
        (mrz.as_str(), "TK0123456", 2.0),
        ("TK0123456", crlf.as_str(), 2.0),
        (mrz.as_str(), crlf.as_str(), 2.0),
        (mrz.as_str(), mismatch.as_str(), 0.0),
        (different_nationality.as_str(), "TK0123456", 2.0),
        (unused_optional.as_str(), "TK0123456", 2.0),
        (used_optional.as_str(), "TK0123456", 2.0),
    ] {
        check(&format!(r#""{left}","{right}","passport-jp""#), CellValue::Number(rank));
    }
    for invalid in [
        mrz.replacen("P<JPN", "P<USA", 1),
        mrz.replacen("P<JPN", "V<JPN", 1),
        mrz.replacen("TK01234568", "TK01234567", 1),
        mrz.replacen("TK01234568", "TKO1234568", 1),
        format!("{UPPER}\nTK01234568JPN9001011<3012316<<<<<<<<<<<<<<03"),
        // The following have a correct composite check digit and a bad field check digit.
        format!("{UPPER}\nTK01234568JPN9001012<3012316<<<<<<<<<<<<<<05"),
        format!("{UPPER}\nTK01234568JPN9001011<3012317<<<<<<<<<<<<<<03"),
        format!("{UPPER}\nTK01234568JPN9001011<3012316A1<<<<<<<<<<<<05"),
        format!("{UPPER}\nTK01234568JPN9001011<3012316A1<<<<<<<<<<<<<5"),
        LOWER.to_owned(),
        format!("{mrz}\nEXTRA"),
        mrz[..mrz.len() - 1].to_owned(),
    ] {
        check(&format!(r#""{invalid}","TK0123456","passport-jp""#), CellValue::Error(CellError::NA));
    }
}

#[test]
fn kana_matches_script_and_width_but_flags_long_vowel_variants() {
    for (args, rank) in [
        (r#""やまだ　たろう","ﾔﾏﾀﾞ ﾀﾛｳ","kana""#, 2.0),
        (r#""ヤマダ タロウ","ヤマダタロウ","kana""#, 2.0),
        (r#""がく","ガク","kana""#, 2.0),
        (r#""ぱぴぷぺぽ","ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ","kana""#, 2.0),
        (r#""ｳﾞｧﾝ","ゔぁん","kana""#, 2.0),
        (r#""アン･マリー","あん・まりー","kana""#, 2.0),
        (r#""ヷ","ヷ","kana""#, 2.0),
        (r#""オオノ","オーノ","kana""#, 1.0),
        (r#""ヤマダ タロー","やまだ　たろう","kana""#, 1.0),
        (r#""ﾔﾏﾀﾞ ﾀﾛｳ","ヤマダ　ジロウ","kana""#, 0.0),
        (r#""キョウコ","キヨウコ","kana""#, 0.0),
        (r#""ガク","カク","kana""#, 0.0),
        (r#""サト ウメ","サトーメ","kana""#, 0.0),
    ] {
        check(args, CellValue::Number(rank));
    }
    for args in [
        r#""山田太郎","やまだたろう","kana""#,
        r#""","やまだ","kana""#,
        r#""ー・","ーー","kana""#,
        r#""カ ﾞ","ガ","kana""#,
        r#""ナﾞ","ナ","kana""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
}

#[test]
fn names_and_readings_preserve_disagreements_and_missing_evidence() {
    for (args, rank) in [
        (r#""山田 太郎","山田太郎","name-with-kana","ﾔﾏﾀﾞ ﾀﾛｳ","やまだたろう""#, 2.0),
        (r#""山田 太郎","山田太郎","name-with-kana","ヤマダ タロー","やまだたろう""#, 1.0),
        (r#""山田 太郎","山田太郎","name-with-kana","ヤマダ タロウ","やまだじろう""#, 0.0),
        (r#""小野 花子","尾野 花子","name-with-kana","オノ ハナコ","オノ ハナコ""#, 1.0),
        (r#""小野花子","尾野花子","name-with-kana","オノハナコ","オノハナコ""#, 0.0),
        (r#""山田太郎","山田太郎","name-with-kana",C1,"ヤマダタロウ""#, 1.0),
        (r#""山田太郎","山田太郎","name-with-kana","ヤマダタロウ","　""#, 1.0),
        (r#""山田太郎","山田太郎","name-with-kana","","""#, 1.0),
        (r#""山田太郎","山本次郎","name-with-kana","","""#, 0.0),
    ] {
        check(args, CellValue::Number(rank));
    }
    for args in [
        r#""山田太郎","山田太郎","name-with-kana","","山田太郎""#,
        r#""山田太郎","山田太郎","name-with-kana",0,"ヤマダタロウ""#,
        r#"C1,"山田太郎","name-with-kana","","ヤマダタロウ""#,
    ] {
        check(args, CellValue::Error(CellError::NA));
    }
    check(r#""山田太郎","山田太郎","name-with-kana","ヤマダタロウ""#, CellValue::Error(CellError::Value));
    check(r#""山田太郎","山田太郎","name","","""#, CellValue::Error(CellError::Value));
}

#[test]
fn record_match_keeps_conflicts_distinct_from_missing_evidence() {
    let sheet = Sheet { date1904: false };
    for (args, expected) in [
        ("2,2,2", CellValue::Number(2.0)),
        ("1,2,2", CellValue::Number(1.0)),
        ("2,#N/A", CellValue::Number(1.0)),
        ("C1,2", CellValue::Number(1.0)),
        (r#"2,"""#, CellValue::Number(1.0)),
        ("#N/A,1,0,2", CellValue::Number(0.0)),
        ("0,#N/A", CellValue::Number(0.0)),
        ("#N/A,C1", CellValue::Error(CellError::NA)),
        ("0,1/0", CellValue::Error(CellError::Div0)),
        ("2,3", CellValue::Error(CellError::Value)),
        (r#"2,"1""#, CellValue::Error(CellError::Value)),
        ("2,TRUE", CellValue::Error(CellError::Value)),
    ] {
        let formula = format!("IBIS.RECORDMATCH({args})");
        let stored = to_file(&formula);
        assert!(stored.starts_with("_xlfn.IBIS.RECORDMATCH("), "{stored}");
        for expression in [&formula, &stored] {
            assert_eq!(evaluate(&sheet, (0, 2, 0), expression), (expected.clone(), false), "{expression}");
        }
    }
    let formula = r#"TEXTJOIN(",",FALSE,IBIS.RECORDMATCH({2,1,0,#N/A},2))"#;
    assert_eq!(evaluate(&sheet, (0, 2, 0), formula), (CellValue::Text("2,1,0,1".into()), false));
}

#[test]
fn record_match_combines_name_date_and_address_formulas() {
    let sheet = Sheet { date1904: false };
    let formula = concat!(
        r#"IF(IBIS.RECORDMATCH(IBIS.FIELDMATCH("鈴木 花子","田中 花子","name"),"#,
        r#"IBIS.FIELDMATCH("1990-12-15","1990-12-15","date"),IBIS.FIELDMATCH(A1,B1,"address"))=1,"要確認","")"#,
    );
    assert_eq!(evaluate(&sheet, (0, 2, 0), formula), (CellValue::Text("要確認".into()), false));
}

#[test]
fn record_match_range_reduces_columns_and_spills_by_row() {
    let sheet = Sheet { date1904: false };
    for (formula, expected) in [
        (
            r#"TEXTJOIN(",",FALSE,IFNA(IBIS.RECORDMATCH({2,2;2,1;2,0;2,#N/A;#N/A,#N/A}),"?"))"#,
            CellValue::Text("2,1,0,1,?".into()),
        ),
        (
            r#"TEXTJOIN(",",FALSE,IBIS.RECORDMATCH({2,1;2,0},{2,2;1,2}))"#,
            CellValue::Text("2,1,1,0".into()),
        ),
        (
            r#"TEXTJOIN(",",FALSE,FILTER({"一致";"要確認";"除外"},IBIS.RECORDMATCH({2,2;2,1;2,0})>0))"#,
            CellValue::Text("一致,要確認".into()),
        ),
        ("IBIS.RECORDMATCH({2,1,2})", CellValue::Number(1.0)),
        ("IBIS.RECORDMATCH(2)", CellValue::Number(2.0)),
        ("IBIS.RECORDMATCH(C1)", CellValue::Error(CellError::NA)),
        ("IBIS.RECORDMATCH(1/0)", CellValue::Error(CellError::Div0)),
    ] {
        assert_eq!(evaluate(&sheet, (0, 2, 0), formula), (expected, false), "{formula}");
    }
}
