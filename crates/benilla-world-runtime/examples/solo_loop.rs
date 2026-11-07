// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure-Rust end-to-end fixture demo. Outputs go only to a new, explicit directory.
use benilla_world_data::{load_json, Limits};
use benilla_world_runtime::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};
const FIXTURE: &[u8] = include_bytes!("../../benilla-world-data/tests/fixtures/original-camp.json");
const PLAYER: PlayerId = PlayerId {
    owner: [1; 16],
    character: [2; 16],
};
fn command(world: &mut World, command: Command) -> Result<(), Box<dyn std::error::Error>> {
    let tick = world.state().tick + 1;
    let input = Input {
        world: world.state().world,
        tick,
        player: PLAYER,
        sequence: world.player(PLAYER).unwrap().next_sequence,
        command,
    };
    let result = world.step(
        tick,
        vec![OwnedInput {
            authenticated_owner: PLAYER.owner,
            input,
        }],
    )?;
    if let Some(rejection) = result.outcomes[0].rejection {
        return Err(format!("command rejected: {rejection:?}").into());
    }
    Ok(())
}
fn wait(world: &mut World, tick: u64) -> Result<(), Error> {
    while world.state().tick < tick {
        world.step(world.state().tick + 1, vec![])?;
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args_os()
        .nth(1)
        .ok_or("usage: solo_loop NEW_OUTPUT_DIRECTORY")?;
    let directory = Path::new(&destination);
    let data = load_json(FIXTURE, Limits::default())?;
    let rules = Rules::original_camp();
    let mut world = World::new(data.clone(), rules.clone(), [3; 16], 42, vec![PLAYER])?;
    let base = world.snapshot_bytes()?;
    let giver = world.npc(1).unwrap().id;
    command(
        &mut world,
        Command::AcceptQuest {
            quest: 910001,
            giver,
        },
    )?;
    for _ in 0..2 {
        command(
            &mut world,
            Command::Move {
                delta: [1000, 0, 0],
            },
        )?;
    }
    for kill in 0..2 {
        if kill > 0 {
            let tick = world.npc(2).unwrap().respawn_at.unwrap();
            wait(&mut world, tick)?;
        }
        let target = world.npc(2).unwrap().id;
        command(&mut world, Command::Melee { target })?;
        let tick = world.state().tick;
        command(
            &mut world,
            Command::Cast {
                ability: 930001,
                target: Target::Npc { id: target },
            },
        )?;
        wait(&mut world, tick + 9)?;
        command(&mut world, Command::Melee { target })?;
        command(
            &mut world,
            Command::Cast {
                ability: 930001,
                target: Target::Npc { id: target },
            },
        )?;
        wait(&mut world, tick + 16)?;
        command(&mut world, Command::OpenLoot { target })?;
        command(&mut world, Command::TakeLootMoney { target })?;
        command(&mut world, Command::TakeLootItem { target, slot: 0 })?;
        command(&mut world, Command::ReleaseLoot { target })?;
    }
    command(
        &mut world,
        Command::TurnInQuest {
            quest: 910001,
            giver,
        },
    )?;
    // Immutable demo outputs cannot overwrite previous saves. A production storage adapter
    // must add atomic checkpoint replacement, recovery and directory durability semantics.
    fs::create_dir(directory)?;
    write_new(&directory.join("genesis.json"), &base)?;
    write_new(&directory.join("journal.json"), &world.journal_bytes()?)?;
    write_new(&directory.join("world.json"), &world.snapshot_bytes()?)?;
    let restored = World::restore_reader(
        data.clone(),
        rules.clone(),
        fs::File::open(directory.join("world.json"))?,
    )?;
    let replayed = World::replay(
        data,
        rules,
        &fs::read(directory.join("genesis.json"))?,
        &fs::read(directory.join("journal.json"))?,
    )?;
    if restored.state_digest() != world.state_digest()
        || replayed.state_digest() != world.state_digest()
    {
        return Err("save/replay verification failed".into());
    }
    let p = restored.player(PLAYER).unwrap();
    println!(
        "Verified solo quest loop: tick={}, XP={}, money={}, item_count={}, quest_turned_in={}",
        restored.state().tick,
        p.xp,
        p.money,
        p.inventory.iter().map(|i| u32::from(i.count)).sum::<u32>(),
        p.quests[&910001].turned_in
    );
    println!("Snapshot reload and full replay have identical canonical state hashes");
    Ok(())
}
