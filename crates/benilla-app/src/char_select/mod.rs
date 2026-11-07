//! Fork-only archived-character recovery. Stock GlueXML owns the roster and character screens.
pub(crate) mod recovery;
pub(crate) mod recovery_model;
use bevy::prelude::*;
pub(crate) fn wow_font(assets: &AssetServer) -> Handle<Font> {
    assets.load("mpq://Fonts/FRIZQT__.ttf")
}

pub(crate) fn class_name(class: u8) -> &'static str {
    match class {
        1 => "Warrior",
        2 => "Paladin",
        3 => "Hunter",
        4 => "Rogue",
        5 => "Priest",
        7 => "Shaman",
        8 => "Mage",
        9 => "Warlock",
        11 => "Druid",
        _ => "?",
    }
}
