// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::*;
#[cfg_attr(test, ::core::prelude::v1::test)]
pub(crate) fn intrinsic_hugs_content_with_standard_insets() {
    let (width, items) = widths(&[12., 84.], Mode::Intrinsic);
    assert_eq!(items, vec![28., 100.]);
    assert_eq!(width, 136.);
}
#[cfg_attr(test, ::core::prelude::v1::test)]
pub(crate) fn fixed_track_uses_natural_content_ratio_and_leaves_no_remainder() {
    let (width, items) = widths(&[12., 84.], Mode::Proportional { width: px(200.) });
    assert_eq!(width, 200.);
    assert_eq!(items, vec![24., 168.]);
    assert_eq!(items.iter().sum::<f32>() + 8., width);
}
#[cfg_attr(test, ::core::prelude::v1::test)]
pub(crate) fn clipping_alignment_depends_on_real_content_width() {
    assert!(centered(false, 40., 40.));
    assert!(!centered(false, 40., 39.));
    assert!(centered(true, 40., 1.));
}
#[cfg_attr(test, ::core::prelude::v1::test)]
pub(crate) fn tiny_or_empty_tracks_are_finite() {
    assert_eq!(
        widths(&[10., 20.], Mode::Proportional { width: px(1.) }).1,
        vec![0., 0.]
    );
    assert!(
        widths(&[], Mode::Proportional { width: px(100.) })
            .1
            .is_empty()
    );
    assert_eq!(
        widths(&[0., 0.], Mode::Proportional { width: px(100.) }).1,
        vec![46., 46.]
    );
}

#[cfg_attr(test, ::core::prelude::v1::test)]
pub(crate) fn icon_uses_the_entire_slot_for_centering_and_clipping() {
    let available = content_width(true, 24.);
    assert_eq!(available, 24.);
    assert!(centered(true, 18., available));
    assert_eq!((available - 18.) / 2. + 18. / 2., 12.);
    assert_eq!(content_width(false, 24.), 8.);
    assert!(!centered(false, 18., content_width(false, 24.)));
}
