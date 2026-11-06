// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issue #84 スライド マスター表示: a layout renamed, copied and deleted in the file.

use ibis_pptx::master_page;

fn file(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn a_layout_and_a_master_are_renamed() {
    let data = file("editing/pptx-plan.pptx");
    let pages = master_page::pages(&data).unwrap();
    let renamed = master_page::rename_page(&data, &pages[2].part, "名前を変えた").unwrap();
    assert_eq!(master_page::pages(&renamed).unwrap()[2].name, "名前を変えた");
    assert!(ibis_pptx::open(renamed.clone()).unwrap().presentation.layouts.iter().any(|l| l.name == "名前を変えた"));
    let master = master_page::rename_page(&renamed, &pages[0].part, "新しいマスター").unwrap();
    assert_eq!(master_page::pages(&master).unwrap()[0].name, "新しいマスター");
    assert!(ibis_pptx::open(master).unwrap().presentation.masters.iter().any(|m| m.name == "新しいマスター"));
}

#[test]
fn a_layout_is_copied_after_itself_and_deleted_again() {
    let data = file("editing/pptx-plan.pptx");
    let before = master_page::pages(&data).unwrap();
    let layouts = ibis_pptx::open(data.clone()).unwrap().presentation.layouts.len();
    let (copied, part) = master_page::duplicate_layout(&data, &before[2].part, "コピー").unwrap();
    let after = master_page::pages(&copied).unwrap();
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after[3].part, part);
    assert_eq!(after[3].name, "コピー");
    assert_eq!(after[2].name, before[2].name);
    // The file opens, with one more layout; the copy is a page of its own that builds.
    assert_eq!(ibis_pptx::open(copied.clone()).unwrap().presentation.layouts.len(), layouts + 1);
    assert!(ibis_pptx::open(master_page::build(&copied, &part).unwrap()).is_ok());
    // A second copy gets a part and an id of its own.
    let (twice, part2) = master_page::duplicate_layout(&copied, &before[2].part, "コピー2").unwrap();
    assert_ne!(part2, part);
    assert_eq!(master_page::pages(&twice).unwrap().len(), before.len() + 2);
    // Deleted: as it was.
    let back = master_page::delete_layout(&copied, &part).unwrap();
    assert_eq!(master_page::pages(&back).unwrap(), before);
    assert_eq!(ibis_pptx::open(back).unwrap().presentation.layouts.len(), layouts);
}

#[test]
fn a_layout_a_slide_follows_is_not_deleted() {
    let data = file("editing/pptx-plan.pptx");
    let pages = master_page::pages(&data).unwrap();
    let used: Vec<_> = pages.iter().filter(|p| !p.master).filter(|p| master_page::layout_in_use(&data, &p.part).unwrap()).collect();
    assert!(!used.is_empty());
    assert!(master_page::delete_layout(&data, &used[0].part).is_err());
    let unused: Vec<_> = pages.iter().filter(|p| !p.master).filter(|p| !master_page::layout_in_use(&data, &p.part).unwrap()).collect();
    assert!(!unused.is_empty());
    assert!(master_page::delete_layout(&data, &unused[0].part).is_ok());
}
