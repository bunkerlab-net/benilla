use std::{collections::BTreeSet, env, fs, path::PathBuf};

// Extract real input variant declarations, including all their fields. This is not a manually
// re-created command enum: changing a used app variant breaks this harness at the same seam.
fn block_body(source: &str, declaration: &str) -> String {
    let tail = &source[source.find(declaration).expect("app declaration")..];
    let start = tail.find('{').unwrap() + 1;
    let mut depth = 1;
    for (i, ch) in tail[start..].char_indices() {
        if ch == '{' {
            depth += 1;
        }
        if ch == '}' {
            depth -= 1;
        }
        if depth == 0 {
            return tail[start..start + i].to_owned();
        }
    }
    panic!("unterminated enum")
}
fn enum_body(source: &str, name: &str) -> String {
    block_body(source, &format!("enum {name} {{"))
}
fn main() {
    // This no-Bevy harness cannot activate the concrete loaded-Wander reopen seam.
    println!("cargo::rustc-check-cfg=cfg(benilla_headless_geometry)");
    println!("cargo::rustc-cfg=benilla_headless_geometry");
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    // Exercise the real whisper state machine without linking the Bevy renderer.
    let mut support = String::new();
    for (path, functions) in [
        ("net/p2p.rs", &["chat", "emit", "character_name"][..]),
        ("net/p2p/character_actions.rs", &["character_guid"][..]),
    ] {
        let path = root.join("crates/benilla-app/src").join(path);
        println!("cargo:rerun-if-changed={}", path.display());
        let source = fs::read_to_string(&path).unwrap();
        if path.ends_with("p2p.rs") {
            support.push_str("pub(crate) ");
            support.push_str(
                source
                    .lines()
                    .find(|line| line.starts_with("const MAX_CHAT:"))
                    .expect("actual chat bound"),
            );
            support.push('\n');
        }
        for name in functions {
            support.push_str("pub(crate) ");
            support.push_str(&function(&source, name));
            support.push('\n');
        }
    }
    let path = root.join("crates/benilla-app/src/ui_unit.rs");
    println!("cargo:rerun-if-changed={}", path.display());
    let source = fs::read_to_string(path).unwrap();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out.join("whisper_support.rs"), support).unwrap();
    fs::write(
        out.join("race_team.rs"),
        format!("pub(crate) {}", function(&source, "race_pvp_team")),
    )
    .unwrap();
    let flags = root.join("crates/benilla-app/src/creature_anim/select.rs");
    println!("cargo:rerun-if-changed={}", flags.display());
    let flags = fs::read_to_string(flags).unwrap();
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("move_flags.rs"),
        block_body(&flags, "mod move_flags {"),
    )
    .unwrap();
    let app = root.join("crates/benilla-app/src/net.rs");
    println!("cargo:rerun-if-changed={}", app.display());
    let source = fs::read_to_string(app).unwrap();
    // Comments can contain braces/commas, so omit them before balanced extraction.
    let source = source
        .lines()
        .filter(|s| !s.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let wanted: BTreeSet<_> = [
        "Move",
        "MoveModeAck",
        "ForceSpeedAck",
        "CancelCast",
        "CastSpell",
        "AttackSwing",
        "AttackStop",
        "QuestgiverAccept",
        "QuestgiverChooseReward",
        "Loot",
        "AutostoreLootItem",
        "LootMoney",
        "LootRelease",
        "SwapInvItem",
        "SplitItem",
        "DestroyItem",
        "ItemQuery",
        "QuestQuery",
        "NpcTextQuery",
        "QuestgiverStatusQuery",
        "QuestgiverHello",
        "QuestgiverQuery",
        "QuestgiverComplete",
        "QuestgiverRequestReward",
        "SetActiveMover",
        "WorldportAck",
        "SetSelection",
        "Logout",
        "ForceLogout",
        "CastSpellAtDest",
        "RepopRequest",
    ]
    .into_iter()
    .collect();
    let body = enum_body(&source, "ClientCommand");
    let mut selected = String::new();
    let mut found = BTreeSet::new();
    let (mut depth, mut start) = (0, 0);
    for (i, ch) in body.char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            _ => {}
        }
        if ch == ',' && depth == 0 {
            let variant = body[start..i].trim();
            start = i + 1;
            let name = variant
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap();
            if wanted.contains(name) {
                found.insert(name);
                selected.push_str(variant);
                selected.push_str(",\n");
            }
        }
    }
    assert_eq!(
        found, wanted,
        "all required declarations must come from actual app source"
    );
    let generated = format!(
        "#[allow(dead_code)] #[derive(Debug)] pub(crate) enum MoveKind {{ {} }}\n#[allow(dead_code)] #[derive(Debug)] pub(crate) enum ClientCommand {{ {} UnsupportedHarnessCommand, }}",
        enum_body(&source, "MoveKind"),
        selected
    );
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("commands.rs"),
        generated,
    )
    .unwrap();
}

fn function(source: &str, name: &str) -> String {
    let declaration = format!("fn {name}(");
    let tail = &source[source.find(&declaration).expect("actual app function")..];
    let brace = tail.find('{').unwrap();
    format!("{}{{{}}}", &tail[..brace], block_body(tail, &declaration))
}
