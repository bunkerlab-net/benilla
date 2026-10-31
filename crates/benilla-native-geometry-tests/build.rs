// SPDX-License-Identifier: GPL-3.0-or-later
// The light geometry harness shares readers but cannot compile the concrete Bevy corridor.
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(benilla_headless_geometry)");
    println!("cargo::rustc-cfg=benilla_headless_geometry");
}
