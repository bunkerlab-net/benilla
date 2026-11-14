// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::Result;
use benilla_native_terrain::{location, Tile};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    data: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    map: u32,
    #[arg(long = "tile", required = true, value_parser = tile)]
    tiles: Vec<Tile>,
}
fn tile(value: &str) -> Result<Tile, String> {
    let (x, y) = value.split_once(',').ok_or("expected X,Y")?;
    Ok(Tile {
        x: x.parse().map_err(|_| "invalid X")?,
        y: y.parse().map_err(|_| "invalid Y")?,
    })
}
fn main() -> Result<()> {
    let args = Args::parse();
    let captured = location::prepare(&args.data, args.map, &args.tiles)?;
    captured.publish(&args.output)?;
    println!(
        "source_epoch={} wmo_precedence_required=true gameplay_admitted=false",
        captured.source_epoch()
    );
    Ok(())
}
