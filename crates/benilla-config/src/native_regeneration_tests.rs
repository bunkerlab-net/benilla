//! Generic configuration representation and real executable-adjacent persistence.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct ConfigFile(PathBuf);
impl ConfigFile {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "benilla-native-rate-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }
    fn path(&self) -> PathBuf {
        self.0.join("config.toml")
    }
    fn load(&self) -> Result<AppConfig, ConfigError> {
        AppConfig::load_for_executable(&self.0.join("benilla"))
    }
    fn write(&self, text: &str) {
        std::fs::write(self.path(), text).unwrap();
    }
}
impl Drop for ConfigFile {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn optional_host_rate_requires_explicit_block_and_required_key() {
    let file = ConfigFile::new();
    assert!(file.load().unwrap().native_regeneration.is_none());
    file.write("");
    assert!(file.load().unwrap().native_regeneration.is_none());
    for text in [
        "[native_regeneration]\n",
        "[native_regeneration]\nother=1\n",
        "[native_regeneration]\nhealth_rate=1\nother=1\n",
        "native_regeneration=1\n",
    ] {
        file.write(text);
        assert!(file.load().is_err(), "{text}");
    }
}
#[test]
fn explicit_host_rates_round_once_and_preserve_signed_zero() {
    let file = ConfigFile::new();
    for (literal, expected) in [
        ("0", 0.0f32),
        ("-0.0", -0.0),
        ("1", 1.0),
        ("1.375", 1.375),
        ("0.1", 0.1),
        ("1000000", 1_000_000.0),
        ("1.1754943508222875e-38", f32::MIN_POSITIVE),
    ] {
        file.write(&format!("[native_regeneration]\nhealth_rate={literal}\n"));
        assert_eq!(
            file.load()
                .unwrap()
                .native_regeneration
                .unwrap()
                .health_rate_bits(),
            expected.to_bits(),
            "{literal}"
        );
    }
}
#[test]
fn invalid_host_rates_fail_closed_without_clamping_or_underflow() {
    let file = ConfigFile::new();
    for literal in [
        "-1",
        "nan",
        "+nan",
        "inf",
        "-inf",
        "1000000.1",
        "1e100",
        "1e-100",
        "1e-40",
        "\"1\"",
        "true",
    ] {
        file.write(&format!("[native_regeneration]\nhealth_rate={literal}\n"));
        assert!(file.load().is_err(), "{literal}");
    }
}
#[test]
fn host_rate_does_not_change_existing_ruleset_fingerprint() {
    let file = ConfigFile::new();
    let base = "[ruleset]\nxp_rate=2.5\ndrop_rate=3.125\n";
    file.write(base);
    let rules = file.load().unwrap().ruleset;
    for rate in ["0", "1", "1.375"] {
        file.write(&format!(
            "{base}\n[native_regeneration]\nhealth_rate={rate}\n"
        ));
        let selected = file.load().unwrap();
        assert_eq!(selected.ruleset, rules);
        assert_eq!(selected.ruleset.fingerprint(), rules.fingerprint());
        assert!(selected.p2p.is_none());
    }
}
#[test]
fn editor_unrelated_save_preserves_explicit_host_rate_and_comments() {
    let file = ConfigFile::new();
    file.write("# local host selection\n[native_regeneration]\nhealth_rate = -0.0 # preserve signed zero\n\n[wow]\ndata = 'Data'\n");
    let before = file.load().unwrap().native_regeneration.unwrap();
    let mut editor = editor::ConfigEditor::open(&file.path()).unwrap();
    editor.set_string("wow", "data", "OtherData");
    let saved = editor.save().unwrap();
    assert_eq!(saved.native_regeneration, Some(before));
    assert_eq!(file.load().unwrap().native_regeneration, Some(before));
    assert_eq!(
        file.load().unwrap().wow.data,
        Some(file.0.join("OtherData"))
    );
    let text = std::fs::read_to_string(file.path()).unwrap();
    assert!(text.contains("# local host selection"));
    assert!(text.contains("health_rate = -0.0 # preserve signed zero"));
}
