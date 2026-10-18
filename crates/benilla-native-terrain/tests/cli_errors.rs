// SPDX-License-Identifier: GPL-3.0-or-later
//! Error reports must be bounded and safe to share, including argument failures.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "native-terrain-cli-private-player-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(cwd: &Path, data: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_benilla-native-terrain"));
    command.current_dir(cwd).arg("--data").arg(data);
    command
}

fn assert_safe_failure(output: Output, exit: i32, id: &str) {
    assert_eq!(output.status.code(), Some(exit));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(
        diagnostic.starts_with(&format!("error[{id}]:")),
        "{diagnostic}"
    );
    assert!(diagnostic.len() < 256);
    assert_eq!(diagnostic.lines().count(), 1);
    assert!(!diagnostic.contains("private-player"));
    assert!(!diagnostic.contains("SECRET"));
    assert!(!diagnostic.contains('/'));
    assert!(!diagnostic.contains('\\'));
}

#[test]
fn missing_data_in_both_modes_never_creates_output() {
    let temp = Temp::new();
    let data = temp.0.join("SECRET-absent-Data");
    let destination = temp.0.join("SECRET-private-export");
    let inspect = command(&temp.0, &data)
        .args(["--inspect", "--map", "1", "--tile", "31,32"])
        .output()
        .unwrap();
    assert_safe_failure(inspect, 1, "source.data_directory_required");
    let export = command(&temp.0, &data)
        .arg("--output")
        .arg(&destination)
        .args(["--map", "1", "--tile", "31,32"])
        .output()
        .unwrap();
    assert_safe_failure(export, 1, "source.data_directory_required");
    assert!(!destination.exists());
    assert!(!data.exists());
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[test]
fn missing_or_conflicting_modes_fail_before_source_access() {
    let temp = Temp::new();
    let data = temp.0.join("SECRET-absent-Data");
    let destination = temp.0.join("SECRET-private-export");
    let missing = command(&temp.0, &data)
        .args(["--map", "1", "--tile", "31,32"])
        .output()
        .unwrap();
    assert_safe_failure(missing, 2, "cli.missing_argument");
    let conflict = command(&temp.0, &data)
        .args(["--inspect", "--output"])
        .arg(&destination)
        .args(["--map", "1", "--tile", "31,32"])
        .output()
        .unwrap();
    assert_safe_failure(conflict, 2, "cli.mode_conflict");
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[test]
fn argument_diagnostics_do_not_echo_private_values() {
    let temp = Temp::new();
    let data = temp.0.join("SECRET-absent-Data");
    for value in [
        "/private-player/SECRET/path",
        "C:\\private-player\\SECRET\\path",
    ] {
        let invalid_map = command(&temp.0, &data)
            .args(["--inspect", "--map", value, "--tile", "31,32"])
            .output()
            .unwrap();
        assert_safe_failure(invalid_map, 2, "cli.invalid_value");
        let invalid_tile = command(&temp.0, &data)
            .args(["--inspect", "--map", "1", "--tile", value])
            .output()
            .unwrap();
        assert_safe_failure(invalid_tile, 2, "cli.invalid_value");
    }
    let unknown = command(&temp.0, &data)
        .args([
            "--inspect",
            "--map",
            "1",
            "--tile",
            "31,32",
            "--SECRET-private-player",
        ])
        .output()
        .unwrap();
    assert_safe_failure(unknown, 2, "cli.unknown_argument");
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[test]
fn malformed_source_diagnostic_hides_archive_and_install_paths() {
    let temp = Temp::new();
    let data = temp.0.join("SECRET-Data");
    fs::create_dir(&data).unwrap();
    let archive = data.join("terrain.MPQ");
    let original = b"not a valid archive: /private-player/SECRET/path";
    fs::write(&archive, original).unwrap();
    let output = command(&temp.0, &data)
        .args(["--inspect", "--map", "1", "--tile", "31,32"])
        .output()
        .unwrap();
    assert_safe_failure(output, 1, "source.open_patch_chain");
    assert_eq!(fs::read(&archive).unwrap(), original);
    assert_eq!(fs::read_dir(&data).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn help_uses_fixed_program_name_instead_of_private_argv_zero() {
    use std::os::unix::process::CommandExt;
    let temp = Temp::new();
    let output = Command::new(env!("CARGO_BIN_EXE_benilla-native-terrain"))
        .arg0("/private-player/SECRET/bin/custom-terrain")
        .arg("--help")
        .current_dir(&temp.0)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("benilla-native-terrain"));
    assert!(help.contains("--inspect"));
    assert!(help.contains("--output"));
    assert!(!help.contains("private-player"));
    assert!(!help.contains("SECRET"));
    assert!(help.len() < 4096);
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}
