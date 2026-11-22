// SPDX-License-Identifier: GPL-3.0-or-later
#[path = "../link_plan.rs"]
mod link_plan;

#[test]
fn linux_native_dependencies_follow_the_archive_on_both_supported_abis() {
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        assert_eq!(
            link_plan::arguments(target, "/out/libbenilla_pinned_generator.a"),
            [
                "/out/libbenilla_pinned_generator.a",
                "-lstdc++",
                "-lm",
                "-lc"
            ]
        );
    }
}

#[test]
fn other_platforms_keep_their_native_runtime_and_no_linux_libraries() {
    assert_eq!(
        link_plan::arguments("aarch64-apple-darwin", "native.a"),
        ["native.a", "-lc++"]
    );
    assert_eq!(
        link_plan::arguments("x86_64-pc-windows-msvc", "native.lib"),
        ["native.lib"]
    );
}
