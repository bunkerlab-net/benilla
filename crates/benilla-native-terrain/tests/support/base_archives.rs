// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared authored-install setup, matching the app and geometry harness fixtures.

/// Seed a complete base installation before callers overwrite selected archives.
pub fn write_base_archives(data: &std::path::Path) {
    let empty = super::archive(&[]);
    for name in benilla_formats::VANILLA_BASE_ORDER {
        std::fs::write(data.join(name), &empty).unwrap();
    }
}
