use super::*;

// Deliberately public test-only key, never a shipped config/default.
const TEST_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
fn config(extra: &str) -> String {
    format!("[p2p]\npsk = '{TEST_KEY}'\nnetwork = 'test-world'\n{extra}")
}
fn fixture_directory() -> PathBuf {
    std::env::temp_dir().join("benilla-config-fixture")
}
fn parse(input: &str) -> Result<AppConfig, ConfigError> {
    AppConfig::parse(input, &fixture_directory())
}

#[test]
fn absent_sections_preserve_legacy_defaults() {
    let config = parse("").unwrap();
    assert!(config.p2p.is_none());
    assert!(config.wow.data.is_none());
    assert_eq!(config.ruleset, Ruleset::default());
}
#[test]
fn parses_and_redacts_secret_everywhere() {
    let config = parse(&config("")).unwrap();
    assert_eq!(config.p2p.as_ref().unwrap().psk_bytes()[0], 1);
    assert!(format!("{config:?}").contains("<redacted>"));
    assert!(!format!("{config:?}").contains(TEST_KEY));
    for input in [
        format!("[p2p]\npsk = '{TEST_KEY}'\nnetwork = 12"),
        format!("[p2p]\npsk = '{TEST_KEY}' ???"),
        format!("[p2p]\npsk = ['{TEST_KEY}']"),
        format!("[p2p]\n{TEST_KEY} = true"),
    ] {
        let error = parse(&input).unwrap_err();
        assert!(!format!("{error} {error:?}").contains(TEST_KEY));
        assert!(error.to_string().contains("source withheld"));
    }
}
#[test]
fn psk_alone_selects_default_network() {
    let config = parse(&format!("[p2p]\npsk='{TEST_KEY}'")).unwrap();
    assert_eq!(config.p2p.unwrap().network, "benilla");
}
#[cfg(unix)]
#[test]
fn dangling_config_symlink_does_not_select_legacy() {
    let directory =
        std::env::temp_dir().join(format!("benilla-config-link-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::os::unix::fs::symlink(directory.join("absent"), directory.join("config.toml")).unwrap();
    assert!(AppConfig::load_for_executable(&directory.join("benilla")).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn unknown_fields_are_errors() {
    for input in [
        "typo = 1",
        "[wow]\ndtaa = 'Data'",
        "[ruleset]\nxp = 2",
        "[surprise]\na=1",
    ] {
        assert!(parse(input).is_err());
    }
    assert!(parse(&config("secret_typo = 1")).is_err());
}
#[test]
fn invalid_psks_are_errors_without_echoing_values() {
    for key in [
        "",
        "REPLACE_WITH_64_HEX_CHARACTERS",
        &"0".repeat(64),
        &"x".repeat(64),
        &"f".repeat(63),
        &"f".repeat(65),
    ] {
        assert!(parse(&config("").replace(TEST_KEY, key)).is_err());
    }
    let upper = parse(&config("").replace(TEST_KEY, &TEST_KEY.to_uppercase())).unwrap();
    assert_eq!(
        upper.p2p.unwrap().psk_bytes(),
        parse(&config("")).unwrap().p2p.unwrap().psk_bytes()
    );
}
#[test]
fn resolves_data_relative_to_executable_not_current_directory() {
    assert_eq!(
        parse("[wow]\ndata = 'WoW/Data'").unwrap().wow.data,
        Some(fixture_directory().join("WoW/Data"))
    );
    let absolute = fixture_directory().join("absolute-wow/Data");
    assert_eq!(
        parse(&format!("[wow]\ndata = '{}'", absolute.display()))
            .unwrap()
            .wow
            .data,
        Some(absolute)
    );
    assert!(parse("[wow]\ndata = ''").is_err());
    assert!(AppConfig::parse("", Path::new("relative")).is_err());
}
#[test]
fn validates_network_and_endpoints() {
    for extra in [
        "listen = '127.0.0.1:0'",
        "listen = '224.0.0.1:7474'",
        "peers = ['0.0.0.0:7474']",
        "peers = ['127.0.0.1:0']",
        "peers = ['224.0.0.1:7474']",
        "peers = ['127.0.0.1:7475','127.0.0.1:7475']",
        "max_peers = 0",
        "max_peers = 65",
        "timeout_ms = 99",
        "timeout_ms = 120001",
        "listen = '127.0.0.1:7474'\npeers = ['127.0.0.1:7474']",
        "max_peers = 1\npeers = ['127.0.0.1:7475','127.0.0.1:7476']",
    ] {
        assert!(parse(&config(extra)).is_err(), "{extra}");
    }
    for network in ["", "contains whitespace", "secret/../network", "réseau"] {
        assert!(parse(&config("").replace("test-world", network)).is_err());
    }
    assert!(parse(&config("listen='[::]:7474'\npeers=['[::1]:7475']")).is_ok());
}
#[test]
fn rates_are_bounded_finite_and_fixed_precision() {
    for value in [
        "nan", "inf", "-inf", "0", "-1", "1000.001", "0.0001", "1.0001",
    ] {
        for field in ["xp_rate", "drop_rate"] {
            assert!(
                parse(&format!("[ruleset]\n{field}={value}")).is_err(),
                "{field}={value}"
            );
        }
    }
    let rates = parse("[ruleset]\nxp_rate=2.0\ndrop_rate=1.125")
        .unwrap()
        .ruleset;
    assert_eq!(rates.xp_milli(), 2000);
    assert_eq!(rates.drop_milli(), 1125);
    assert_eq!(rates.xp_rate(), 2.0);
    assert_eq!(rates.drop_rate(), 1.125);
}
#[test]
fn fingerprint_is_canonical_and_sensitive_only_to_rules() {
    let expected = Ruleset::default().fingerprint();
    assert_eq!(
        expected,
        parse("[ruleset]\nxp_rate=1\ndrop_rate=1.0")
            .unwrap()
            .ruleset
            .fingerprint()
    );
    assert_eq!(
        expected,
        parse(&config("[wow]\ndata = '/some/other/path'"))
            .unwrap()
            .ruleset
            .fingerprint()
    );
    assert_ne!(
        expected,
        parse("[ruleset]\nxp_rate=2").unwrap().ruleset.fingerprint()
    );
    assert_ne!(
        expected,
        parse("[ruleset]\ndrop_rate=2")
            .unwrap()
            .ruleset
            .fingerprint()
    );
    // Domain + version(u32 BE) + xp_milli(u32 BE) + drop_milli(u32 BE).
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(b"benilla-ruleset\0\0\0\0\x01\0\0\x03\xe8\0\0\x03\xe8")
        ),
        expected
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
}
#[test]
fn discovers_only_beside_executable_and_rejects_bad_files() {
    let directory =
        std::env::temp_dir().join(format!("benilla-config-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let executable = directory.join("benilla");
    let path = directory.join("config.toml");
    assert!(AppConfig::load_for_executable(&executable)
        .unwrap()
        .p2p
        .is_none());
    std::fs::write(&path, "[wow]\ndata='Data'").unwrap();
    assert_eq!(
        AppConfig::load_for_executable(&executable)
            .unwrap()
            .wow
            .data,
        Some(directory.join("Data"))
    );
    std::fs::write(&path, "[p2p]").unwrap();
    assert!(AppConfig::load_for_executable(&executable).is_err());
    std::fs::write(&path, "#".repeat(MAX_CONFIG_BYTES as usize + 1)).unwrap();
    assert!(AppConfig::load_for_executable(&executable)
        .unwrap_err()
        .to_string()
        .contains("64 KiB"));
    std::fs::write(&path, [0xff]).unwrap();
    assert!(AppConfig::load_for_executable(&executable)
        .unwrap_err()
        .to_string()
        .contains("UTF-8"));
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(AppConfig::load_for_executable(&executable).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn config_limits_match_peer_transport_contract() {
    assert!(parse(&config("timeout_ms=60000")).is_ok());
    assert!(parse(&config("timeout_ms=60001")).is_err());
    let key = "ff".repeat(32);
    assert!(parse(&format!("[p2p]\npsk='{key}'")).is_err());
}
