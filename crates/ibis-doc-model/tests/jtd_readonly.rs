// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_doc_model::{Command, Document, EditError, Presentation};

#[test]
fn readonly_document_rejects_changes_and_new_document_is_editable() {
    let mut doc = Document::new(Presentation::new());
    doc.set_read_only(true);
    let command = Command::SetThemeEffects { name: Some("test".into()) };
    assert!(matches!(doc.execute(vec![command.clone()]), Err(EditError::ReadOnly)));
    assert!(doc.presentation().theme_effects.is_none());
    let mut next = Document::new(Presentation::new());
    assert!(next.execute(vec![command]).is_ok());
}
