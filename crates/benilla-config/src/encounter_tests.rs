use super::*;
fn input(extra: &str) -> String {
    format!(
        r#"
[p2p]
psk = "test-only synthetic encounter phrase"
listen = "127.0.0.1:7474"
{extra}
[p2p.encounter]
local_id = "01010101010101010101010101010101"
epoch = "0101010101010101010101010101010101010101010101010101010101010101"
roster = [{{id="02020202020202020202020202020202", address="127.0.0.1:7475"}}, {{id="01010101010101010101010101010101", address="127.0.0.1:7474"}}]
"#
    )
}
fn parse(s: &str) -> Result<AppConfig, ConfigError> {
    AppConfig::parse(s, Path::new("/tmp"))
}
#[test]
fn canonical_roster_and_default_exploration() {
    let cfg = parse(&input("")).unwrap().p2p.unwrap().encounter.unwrap();
    assert_eq!(cfg.roster[0].id, [1; 16]);
    assert_eq!(cfg.local_id, [1; 16]);
    assert_eq!(cfg.tick_limit, 1200);
    assert!(parse(input("").split("[p2p.encounter]").next().unwrap())
        .unwrap()
        .p2p
        .unwrap()
        .encounter
        .is_none());
}
#[test]
fn malformed_rosters_fail_before_networking() {
    let original = input("");
    for invalid in [
        original.replace(
            "02020202020202020202020202020202",
            "01010101010101010101010101010101",
        ),
        original.replace("127.0.0.1:7475", "127.0.0.1:7474"),
        original.replace("127.0.0.1:7475", "0.0.0.0:7475"),
        original.replace("127.0.0.1:7475", "127.0.0.1:0"),
        original.replace(
            "local_id = \"01010101010101010101010101010101\"",
            "local_id = \"03030303030303030303030303030303\"",
        ),
        original.replace("listen = \"127.0.0.1:7474\"", "listen = \"127.0.0.1:8888\""),
        original.replace(
            "0101010101010101010101010101010101010101010101010101010101010101",
            &"0".repeat(64),
        ),
        input("peers = [\"127.0.0.1:7475\"]"),
        original + "tick_limit = 0\n",
    ] {
        assert!(parse(&invalid).is_err(), "unexpected valid roster");
    }
}

#[test]
fn iroh_and_encounter_cannot_silently_select_one_backend() {
    let error = parse(&(input("") + "\n[p2p.iroh]\n")).unwrap_err();
    assert!(error.to_string().contains("cannot be combined"));
}
