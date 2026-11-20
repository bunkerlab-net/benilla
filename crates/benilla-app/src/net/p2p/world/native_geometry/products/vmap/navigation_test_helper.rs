// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::{Path, PathBuf};

const BUILD_FIRST: &str = "build benilla-native-nav-generator before scoped tests";

#[track_caller]
pub(super) fn helper() -> PathBuf {
    let executable = std::env::current_exe().expect("resolve current test executable");
    find_helper(&executable).unwrap_or_else(|message| panic!("{message}"))
}

fn find_helper(executable: &Path) -> Result<PathBuf, String> {
    let profile = cargo_profile(executable)
        .ok_or_else(|| format!("unsupported Cargo test layout: {}", executable.display()))?;
    let helper = profile.join(format!(
        "benilla-native-navigation{}",
        std::env::consts::EXE_SUFFIX
    ));
    if helper
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.is_file())
    {
        Ok(helper)
    } else {
        Err(format!("{BUILD_FIRST}: {}", helper.display()))
    }
}

fn cargo_profile(executable: &Path) -> Option<&Path> {
    let output = executable.parent()?;
    match output.file_name()?.to_str()? {
        "deps" => output.parent(),
        // Cargo 1.100 puts unit tests in <profile>/build/<package>/<hash>/out.
        "out" => {
            let build = output.ancestors().nth(3)?;
            if build.file_name()? == "build" {
                build.parent()
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    const OLD_TEST: &str = "deps/benilla_native_geometry_tests-9d86a665d2fa88de";
    const NEW_TEST: &str = "build/benilla-native-geometry-tests/2e0c7af9f48c859f/out/benilla_native_geometry_tests-2e0c7af9f48c859f";
    const NAME: &str = "benilla-native-navigation";

    struct TestTree(PathBuf);

    impl TestTree {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "benilla-navigation-layout-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }

        fn executable(&self, relative: &str) -> PathBuf {
            let path = self
                .0
                .join(format!("{relative}{}", std::env::consts::EXE_SUFFIX));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, []).unwrap();
            path
        }
    }

    impl Drop for TestTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_profile_helper_in_cargo_199_layout() {
        let tree = TestTree::new();
        for profile in ["target/debug", "target/aarch64-unknown-linux-gnu/debug"] {
            let executable = tree.executable(&format!("{profile}/{OLD_TEST}"));
            let helper = tree.executable(&format!("{profile}/{NAME}"));
            assert_eq!(find_helper(&executable), Ok(helper));
        }
    }

    #[test]
    fn finds_profile_helper_in_cargo_1100_layout() {
        let tree = TestTree::new();
        for profile in ["target/release", "target/x86_64-unknown-linux-gnu/release"] {
            let executable = tree.executable(&format!("{profile}/{NEW_TEST}"));
            let helper = tree.executable(&format!("{profile}/{NAME}"));
            assert_eq!(find_helper(&executable), Ok(helper));
        }
    }

    #[test]
    fn missing_helper_does_not_use_another_profile_or_target() {
        let tree = TestTree::new();
        let profile = "target/x86_64-unknown-linux-gnu/debug";
        let executable = tree.executable(&format!("{profile}/{NEW_TEST}"));
        for other in [
            "target",
            "target/debug",
            "target/x86_64-unknown-linux-gnu",
            "target/x86_64-unknown-linux-gnu/release",
            "target/aarch64-unknown-linux-gnu/debug",
        ] {
            tree.executable(&format!("{other}/{NAME}"));
        }
        let error = find_helper(&executable).unwrap_err();
        assert!(error.contains(BUILD_FIRST), "{error}");
        let expected = tree
            .0
            .join(profile)
            .join(format!("{NAME}{}", std::env::consts::EXE_SUFFIX));
        assert!(error.contains(expected.to_str().unwrap()), "{error}");
    }

    #[test]
    fn unknown_layout_reports_test_executable() {
        let tree = TestTree::new();
        let executable = tree.executable("target/debug/objects/benilla_native_geometry_tests");
        tree.executable(&format!("target/debug/{NAME}"));
        let error = find_helper(&executable).unwrap_err();
        assert!(error.contains("unsupported Cargo test layout"), "{error}");
        assert!(error.contains(executable.to_str().unwrap()), "{error}");
    }
}
