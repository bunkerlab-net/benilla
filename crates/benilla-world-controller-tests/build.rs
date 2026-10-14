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
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
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
        "ItemQuery",
        "QuestQuery",
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
    let generated = format!("#[allow(dead_code)] #[derive(Debug)] pub(crate) enum MoveKind {{ {} }}\n#[allow(dead_code)] #[derive(Debug)] pub(crate) enum ClientCommand {{ {} UnsupportedHarnessCommand, }}", enum_body(&source, "MoveKind"), selected);
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("commands.rs"),
        generated,
    )
    .unwrap();
}
