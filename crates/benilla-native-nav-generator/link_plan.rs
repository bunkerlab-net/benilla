// SPDX-License-Identifier: GPL-3.0-or-later
//! Binary-only native linkage; never propagated into the protocol library.
pub fn arguments(target: &str, archive: &str) -> Vec<String> {
    let mut args = vec![archive.to_owned()];
    if target.ends_with("linux-gnu") {
        // Rust's earlier system libraries precede our late native archive. Repeat
        // its dependencies afterwards, so --as-needed also sees stack-protector
        // references before processing libc's linker script/loader dependency.
        args.extend(["-lstdc++", "-lm", "-lc"].map(str::to_owned));
    } else if target.contains("apple") {
        args.push("-lc++".to_owned());
    }
    args
}
