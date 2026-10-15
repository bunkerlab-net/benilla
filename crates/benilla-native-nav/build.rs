// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    println!("cargo:rustc-check-cfg=cfg(benilla_native_detour)");
    println!("cargo:rerun-if-changed=native");
    let target = std::env::var("TARGET").unwrap();
    if !matches!(
        target.as_str(),
        "x86_64-unknown-linux-gnu"
            | "aarch64-unknown-linux-gnu"
            | "x86_64-pc-windows-msvc"
            | "aarch64-apple-darwin"
    ) {
        return;
    }
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .define("DT_POLYREF64", "1")
        .include("native/Detour/Include");
    if build.get_compiler().is_like_msvc() {
        build.flag("/fp:strict");
    } else {
        build
            .flag("-fno-fast-math")
            .flag("-ffp-contract=off")
            .flag("-fno-exceptions");
    }
    for file in [
        "DetourAlloc",
        "DetourAssert",
        "DetourCommon",
        "DetourNavMesh",
        "DetourNavMeshQuery",
        "DetourNode",
    ] {
        build.file(format!("native/Detour/Source/{file}.cpp"));
    }
    build
        .file("native/bridge.cpp")
        .file("native/source_smoothing.cpp")
        .file("native/source_direct.cpp")
        .file("native/source_warm_cache.cpp")
        .file("native/source_point_path.cpp")
        .file("native/source_straight_path.cpp")
        .compile("benilla_pinned_detour");
    println!("cargo:rustc-cfg=benilla_native_detour");
}
