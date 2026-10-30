// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

// Run the fork's production placement tests during the application's normal
// test suite, since patched dependencies are not workspace test targets.
#[path = "../../../forks/gpui-component/src/menu/submenu_placement.rs"]
mod placement;
