// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! CMYK colours in the editor (ibisOffice's own; PowerPoint has none): the converter in use and the conversions are
//! `ibis_office_base::cmyk`; what is here is what the window asks of them (a file with the CMYK mode on).

use ibis_doc_model::Rgb;

pub use ibis_office_base::cmyk::*;

use super::editor::SlideEditor;

impl SlideEditor {
    /// Whether the screen's format has the CMYK mode (a presentation) and its file has it on.
    pub(crate) fn cmyk_on(&self) -> bool {
        self.has_cmyk() && self.presentation().cmyk.enabled
    }

    /// The colour to save for a chosen `rgb`: as printed in a file with the CMYK mode on, else as chosen.
    pub(crate) fn chosen_color(&self, rgb: Rgb) -> Rgb {
        if self.cmyk_on() { gamut(rgb) } else { rgb }
    }
}
