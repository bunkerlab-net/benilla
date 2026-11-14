// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(benilla_native_generator)]
use std::{path::Path, process::Command};

#[test]
fn stable_bv_order_and_allocation_failures_use_the_actual_builder() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temporary = std::env::temp_dir().join(format!("benilla-stable-bv-{}", std::process::id()));
    std::fs::create_dir_all(&temporary).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(temporary.clone());
    let executable = temporary.join(format!("stable-bv{}", std::env::consts::EXE_SUFFIX));
    let compiler = cc::Build::new()
        .cargo_metadata(false)
        .target(env!("BENILLA_NATIVE_TEST_TARGET"))
        .host(env!("BENILLA_NATIVE_TEST_TARGET"))
        .opt_level(0)
        .debug(false)
        .cpp(true)
        .std("c++20")
        .define("DT_POLYREF64", "1")
        .include(root.join("native/Detour/Include"))
        .get_compiler();
    let mut build = compiler.to_command();
    build.current_dir(&temporary);
    if compiler.is_like_msvc() {
        build
            .arg("/UNDEBUG")
            .arg("/EHsc")
            .arg(format!("/Fe{}", executable.display()));
    } else {
        build
            .arg("-UNDEBUG")
            .arg("-fexceptions")
            .arg("-o")
            .arg(&executable);
    }
    build.arg(root.join("tests/stable_bv.cpp"));
    for source in ["DetourAlloc.cpp", "DetourAssert.cpp", "DetourCommon.cpp"] {
        build.arg(root.join("native/Detour/Source").join(source));
    }
    let result = build.output().unwrap();
    assert!(
        result.status.success(),
        "builder regression compile failed: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    let fixtures = root.join("../../tools/native-navigation/fixtures");
    let result = Command::new(executable)
        .args(
            [
                "ridge_wmo/0003130.mmtile",
                "ridge_neighbors_wmo/0003130.mmtile",
                "ridge_xminus_wmo/0003130.mmtile",
                "liquid_full_types_wmo/0003130.mmtile",
                "two-tile-seam/0003130.mmtile",
                "two-tile-seam/0003129.mmtile",
            ]
            .map(|path| fixtures.join(path)),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "builder regression failed: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
}
