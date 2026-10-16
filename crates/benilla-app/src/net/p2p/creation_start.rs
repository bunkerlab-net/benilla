//! Local creation policy selection; saved-character entry never calls this module.
use benilla_profile::{
    starting_location::ClassicStart, validate_race_class, CharacterId, Error, NewCharacter,
    ProfileStore,
};
use std::path::Path;

pub(super) const LEGACY_WARNING: &str = "P2P racial starts are not configured: new characters use the legacy Northshire meeting point. Set p2p.classic_start_captures to your private source-v2 capture directory for racial starts. Saved characters keep their positions.";

pub(super) enum CreationStart {
    Legacy,
    Captured(ClassicStart),
}
impl CreationStart {
    pub(super) fn load(directory: Option<&Path>, race: u8, class: u8) -> anyhow::Result<Self> {
        validate_race_class(race, class)?;
        let Some(directory) = directory else {
            return Ok(Self::Legacy);
        };
        if !directory.is_absolute() {
            anyhow::bail!("Classic start directory must be absolute after startup configuration");
        }
        let name = format!("{race}-{class}.json");
        // Only validated numeric IDs choose the filename; no character name or peer input path.
        let start = ClassicStart::read_path(&directory.join(&name)).map_err(|_| anyhow::anyhow!(
            "Cannot use Classic start capture {name}: check p2p.classic_start_captures, local file access and source-v2 validation. Creation refused; saved characters remain available."
        ))?;
        start.position_for(race, class).map_err(|_| anyhow::anyhow!(
            "Classic start capture {name} selects a different race/class. Creation refused; saved characters remain available."
        ))?;
        Ok(Self::Captured(start))
    }
    pub(super) fn create(
        self,
        store: &mut ProfileStore,
        input: NewCharacter,
    ) -> Result<CharacterId, Error> {
        match self {
            Self::Legacy => store.create_character(input),
            Self::Captured(start) => store.create_character_from_start(input, &start),
        }
    }
}

/// First and resumed entry use only the persisted pose; a missing capture cannot move it.
pub(super) fn worldport(character: &benilla_profile::Character) -> benilla_protocol::SessionEvent {
    benilla_protocol::SessionEvent::Worldport {
        map_id: character.position.map_id,
        position: character.position.position,
        orientation: character.position.orientation,
        needs_ack: false,
    }
}
