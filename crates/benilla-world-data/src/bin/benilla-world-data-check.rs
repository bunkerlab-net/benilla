// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_world_data::{load_reader, Limits};
use std::{env, fs::File, process::ExitCode};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: benilla-world-data-check <local-export.json>")?;
    if args.next().is_some() {
        return Err("usage: benilla-world-data-check <local-export.json>".into());
    }
    let world = load_reader(File::open(path)?, Limits::default())?;
    let digest: String = world
        .content_digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let data = world.export();
    println!("Validated data model only; no gameplay or provenance verification");
    println!("profile={} sha256={digest}", data.profile);
    println!(
        "creatures={} spawns={} quests={} relations={} items={} abilities={}",
        data.creatures.len(),
        data.spawns.len(),
        data.quests.len(),
        data.quest_relations.len(),
        data.items.len(),
        data.abilities.len()
    );
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
