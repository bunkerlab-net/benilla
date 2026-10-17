use super::*;

// Deliberately public test-only phrase, never a shipped config/default.
const TEST_PASSPHRASE: &str = "public test phrase only never use privately";
fn config(extra: &str) -> String {
    format!("[p2p]\npsk = '{TEST_PASSPHRASE}'\nnetwork = 'test-world'\n{extra}")
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
    assert_ne!(config.p2p.as_ref().unwrap().psk_bytes(), &[0; 32]);
    assert!(format!("{config:?}").contains("<redacted>"));
    assert!(!format!("{config:?}").contains(TEST_PASSPHRASE));
    for input in [
        format!("[p2p]\npsk = '{TEST_PASSPHRASE}'\nnetwork = 12"),
        format!("[p2p]\npsk = '{TEST_PASSPHRASE}' ???"),
        format!("[p2p]\npsk = ['{TEST_PASSPHRASE}']"),
        format!("[p2p]\n{TEST_PASSPHRASE} = true"),
    ] {
        let error = parse(&input).unwrap_err();
        assert!(!format!("{error} {error:?}").contains(TEST_PASSPHRASE));
        assert!(error.to_string().contains("source withheld"));
    }
}
#[test]
fn psk_alone_selects_default_network() {
    let config = parse(&format!("[p2p]\npsk='{TEST_PASSPHRASE}'")).unwrap();
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
fn oversized_passphrases_are_errors_without_echoing_values() {
    for phrase in ["a".repeat(1025), "é".repeat(513)] {
        let error = parse(&config("").replace(TEST_PASSPHRASE, &phrase)).unwrap_err();
        assert!(!format!("{error} {error:?}").contains(&phrase));
    }
}
#[test]
fn all_passphrase_lengths_and_contents_are_accepted_but_key_is_required() {
    for phrase in [
        "",
        "x",
        " ",
        "too short",
        "xxxxxxxxxxxxxxxx",
        "REPLACE_WITH_A_PRIVATE_RANDOM_PASSPHRASE",
    ] {
        assert!(parse(&config("").replace(TEST_PASSPHRASE, phrase)).is_ok());
    }
    assert!(parse("[p2p]").is_err());
}
#[test]
fn case_whitespace_unicode_and_network_are_exact_key_inputs() {
    let base = derive_psk(TEST_PASSPHRASE, "test-world").unwrap();
    assert_eq!(base.0, derive_psk(TEST_PASSPHRASE, "test-world").unwrap().0);
    for phrase in [
        TEST_PASSPHRASE.to_uppercase(),
        format!(" {TEST_PASSPHRASE}"),
        format!("{TEST_PASSPHRASE} "),
        format!("{TEST_PASSPHRASE}é"),
    ] {
        assert_ne!(base.0, derive_psk(&phrase, "test-world").unwrap().0);
    }
    assert_ne!(base.0, derive_psk(TEST_PASSPHRASE, "Test-world").unwrap().0);
    // Visually equivalent composed/decomposed Unicode is intentionally not normalized.
    assert_ne!(
        derive_psk("public test phrase café", "test-world")
            .unwrap()
            .0,
        derive_psk("public test phrase cafe\u{301}", "test-world")
            .unwrap()
            .0
    );
}
#[test]
fn passphrase_resource_limit_counts_utf8_bytes() {
    assert!(derive_psk("fifteen chars!!", "test-world").is_ok());
    assert!(derive_psk("sixteen chars!!!", "test-world").is_ok());
    assert!(derive_psk(&format!("a{}", "é".repeat(511)), "test-world").is_ok());
    assert!(derive_psk(&format!("ab{}", "é".repeat(511)), "test-world").is_ok());
    assert!(derive_psk(&format!("abc{}", "é".repeat(511)), "test-world").is_err());
}
#[test]
fn hexadecimal_strings_are_passphrases_not_raw_keys() {
    let key = derive_psk(&"0123456789abcdef".repeat(4), "test-world").unwrap();
    let raw = std::array::from_fn(|i| [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef][i % 8]);
    assert_ne!(key.0, raw);
}
#[test]
fn sample_parses_but_its_public_phrase_offers_no_privacy() {
    assert!(parse(include_str!("../../../config.example.toml")).is_ok());
}
#[test]
fn kdf_parameters_cannot_be_overridden_in_config() {
    for field in [
        "kdf_version=2",
        "memory_kib=1",
        "iterations=1",
        "parallelism=1",
        "salt='custom'",
    ] {
        assert!(parse(&config(field)).is_err());
    }
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
    let config = parse(&format!("[p2p]\npsk='{key}'")).unwrap();
    assert_ne!(config.p2p.unwrap().psk_bytes(), &[0xff; 32]);
}

#[test]
fn passphrase_kdf_matches_independent_reference_vector() {
    // Public fixture, cross-checked with the upstream C Argon2 implementation via
    // argon2-cffi's hash_secret_raw(..., time_cost=3, memory_cost=65536,
    // parallelism=4, hash_len=32, type=Type.ID, version=19).
    assert_eq!(PASSPHRASE_KDF_VERSION, 1);
    assert_eq!(
        passphrase_salt("test-world"),
        [
            0x07, 0x14, 0x1e, 0xac, 0xa2, 0x5e, 0x62, 0x13, 0x79, 0x98, 0x0c, 0x39, 0x44, 0xda,
            0xe7, 0x46
        ]
    );
    assert_eq!(
        derive_psk(TEST_PASSPHRASE, "test-world").unwrap().0,
        [
            0x81, 0x70, 0x5a, 0x84, 0x98, 0xfd, 0x02, 0xbf, 0x37, 0x08, 0xab, 0x48, 0xc2, 0x06,
            0x4f, 0xb9, 0x09, 0x82, 0xac, 0x15, 0xc3, 0x5c, 0xef, 0x43, 0xa6, 0xee, 0x28, 0xbe,
            0x75, 0x6d, 0xd6, 0xbd
        ]
    );
}

#[test]
fn iroh_mode_is_opt_in_and_router_mapping_defaults_off() {
    let normal = parse(&config("")).unwrap().p2p.unwrap();
    assert!(normal.internet.is_none());
    let internet = parse(&config("[p2p.iroh]\n"))
        .unwrap()
        .p2p
        .unwrap()
        .internet
        .unwrap();
    assert!(!internet.port_mapping);
    assert!(internet.relay_urls.is_empty());
    assert!(internet.peer_tickets.is_empty());
}

#[test]
fn iroh_validation_rejects_mixed_modes_and_bad_infrastructure() {
    for extra in [
        "peers=['127.0.0.1:7475']\n[p2p.iroh]",
        "[p2p.iroh]\nrelay_urls=['http://example.com']",
        "[p2p.iroh]\npeer_tickets=['not a ticket']",
        "[p2p.iroh]\nunknown_field=true",
    ] {
        assert!(parse(&config(extra)).is_err(), "accepted {extra}");
    }
}

#[test]
fn exploration_timezone_defaults_validates_and_never_echoes_bad_input() {
    assert_eq!(
        parse(&config("")).unwrap().p2p.unwrap().timezone.name(),
        "UTC"
    );
    assert_eq!(
        parse(&config("timezone = 'Africa/Johannesburg'"))
            .unwrap()
            .p2p
            .unwrap()
            .timezone
            .name(),
        "Africa/Johannesburg"
    );
    for bad in ["", "local", "africa/johannesburg", "UTC+2", TEST_PASSPHRASE] {
        let error = parse(&config(&format!("timezone = '{bad}'"))).unwrap_err();
        assert!(error.to_string().contains("source withheld"));
        assert!(!error.to_string().contains(TEST_PASSPHRASE));
    }
}

#[test]
fn classic_start_capture_directory_is_optional_and_executable_relative_without_io() {
    assert!(parse(&config(""))
        .unwrap()
        .p2p
        .unwrap()
        .classic_start_captures
        .is_none());
    let parsed = parse(&config(
        "classic_start_captures = 'benilla-config/classic-starts'",
    ))
    .unwrap();
    assert_eq!(
        parsed.p2p.unwrap().classic_start_captures,
        Some(fixture_directory().join("benilla-config/classic-starts"))
    );
    let absolute = fixture_directory().join("missing-start-captures");
    let field = format!(
        "classic_start_captures = {}",
        toml::Value::String(absolute.to_string_lossy().into_owned())
    );
    assert_eq!(
        parse(&config(&field))
            .unwrap()
            .p2p
            .unwrap()
            .classic_start_captures,
        Some(absolute)
    );
    for invalid in [
        "classic_start_captures = ''",
        "classic_start_captures = 42",
        "classic_start_captures = ['unexpected']",
    ] {
        assert!(parse(&config(invalid)).is_err());
    }
}

#[test]
fn legacy_internet_alias_loads_identically_and_rejects_both_spellings() {
    let canonical = parse(&config("[p2p.iroh]\nport_mapping = true")).unwrap();
    let legacy = parse(&config("[p2p.internet]\nport_mapping = true")).unwrap();
    assert_eq!(
        canonical.p2p.unwrap().internet.unwrap().port_mapping,
        legacy.p2p.unwrap().internet.unwrap().port_mapping
    );
    for tables in ["[p2p.iroh]\n[p2p.internet]", "[p2p.internet]\n[p2p.iroh]"] {
        let error = parse(&config(tables)).unwrap_err().to_string();
        assert!(error.contains("source withheld"));
        assert!(!error.contains(TEST_PASSPHRASE));
    }
}
