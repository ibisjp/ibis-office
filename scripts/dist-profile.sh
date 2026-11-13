#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Sourced by the scripts that build what is distributed (bundle-macos.sh, bundle-linux.sh, bundle-flatpak.sh, build-web.sh;
# bundle-windows.ps1 sets the same two variables), before their `cargo build`.
#
# The root Cargo.toml's [profile.release] is the development setting (incremental on: a rebuild after a change is about 40
# percent faster, tasks/doc/reports/2026-11-09-mac-build-timing.md). What is distributed is built as release always was: no
# incremental compilation and Cargo's default LTO (`false`: thin-local, within each crate; set explicitly so that a later change
# of the profile cannot reach it). Environment variables override the profile, so the distribution keeps its settings without a
# second profile (and a second target folder).
# scripts/check-dist-profile.sh checks that every such script sets them before it builds.
export CARGO_PROFILE_RELEASE_INCREMENTAL=false
export CARGO_PROFILE_RELEASE_LTO=false
# What is distributed does not show the lines of the menu design that are not made yet (grey, "(開発中)"): the app reads this at
# compile time (`show_planned` in crates/ibis-office-shell/src/menu_layout.rs; cargo builds the crate again when it changes).
# A development build (no variable) shows them. IBIS_OFFICE_PLANNED=1 / 0 in the environment of the running app overrides either.
export IBIS_OFFICE_DIST=1
