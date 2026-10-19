// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rustc-check-cfg=cfg(benilla_native_generator)");
    let target = std::env::var("TARGET").unwrap();
    if !matches!(
        target.as_str(),
        "x86_64-unknown-linux-gnu"
            | "aarch64-unknown-linux-gnu"
            | "x86_64-pc-windows-msvc"
            | "aarch64-apple-darwin"
    ) {
        // Protocol consumers remain portable; the executable reports unsupported.
        return;
    }
    println!("cargo:rustc-cfg=benilla_native_generator");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .cargo_metadata(false)
        .define("DT_POLYREF64", "1")
        .include("native/Recast/Include")
        .include("native/Detour/Include");
    let msvc = build.get_compiler().is_like_msvc();
    if msvc {
        build.flag("/fp:strict").flag("/EHsc");
    } else {
        build
            .flag("-fno-fast-math")
            .flag("-ffp-contract=off")
            .flag("-fexceptions");
    }
    for directory in ["Recast", "Detour"] {
        let mut sources: Vec<_> = std::fs::read_dir(format!("native/{directory}/Source"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|s| s == "cpp"))
            .collect();
        sources.sort();
        for source in sources {
            build.file(source);
        }
    }
    build
        .file("native/generator.cpp")
        .compile("benilla_pinned_generator");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let archive = out.join(if msvc {
        "benilla_pinned_generator.lib"
    } else {
        "libbenilla_pinned_generator.a"
    });
    // Binary-only link arguments deliberately do not propagate through the protocol-only rlib.
    println!(
        "cargo:rustc-link-arg-bin=benilla-native-navigation={}",
        archive.display()
    );
    if !msvc {
        println!(
            "cargo:rustc-link-arg-bin=benilla-native-navigation=-l{}",
            if target.contains("apple") {
                "c++"
            } else {
                "stdc++"
            }
        );
    }
}
