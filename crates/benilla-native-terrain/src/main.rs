// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::Result;
use benilla_native_terrain::{prepare, Tile};
use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    about = "Produce selected dry terrain privately from your own Classic Data; no gameplay activation"
)]
struct Args {
    #[arg(long)]
    data: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    map: u32,
    /// Terrain grid pair X,Y; repeat at most nine times. ADT filename order is reversed.
    #[arg(long = "tile", required = true, value_parser = parse_tile)]
    tiles: Vec<Tile>,
}
fn parse_tile(s: &str) -> Result<Tile, String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y")?;
    Ok(Tile {
        x: x.parse().map_err(|_| "invalid X")?,
        y: y.parse().map_err(|_| "invalid Y")?,
    })
}
fn main() -> Result<()> {
    let args = Args::parse();
    let prepared = prepare(&args.data, args.map, &args.tiles)?;
    let count = prepared.tiles().count();
    prepared.publish(&args.output)?;
    println!("Produced {count} dry terrain tiles; no live installation lease or gameplay admission established");
    Ok(())
}
