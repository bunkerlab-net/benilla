// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(not(benilla_native_generator))]

#[test]
fn unsupported_target_reports_a_clean_error() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_benilla-native-navigation"))
        .output()
        .expect("the unsupported-target executable must still build");
    assert_eq!(result.status.code(), Some(3));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unsupported on this target"));
}
