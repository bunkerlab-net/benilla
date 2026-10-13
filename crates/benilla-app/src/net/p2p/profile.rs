//! Local character lobby over the existing character-screen command/event bridge.
//! No transport is opened until a saved character is explicitly entered.
use super::*;
use benilla_profile::{Appearance, Character, NewCharacter, ProfileStore};
use benilla_protocol::{CharAction, Character as RosterCharacter, RosterCause};
use std::path::Path;

/// A small, immutable snapshot of the actual install's character-creation DBCs.
/// Peers send appearance choices, never display IDs or faction templates to trust.
#[derive(Clone, Default)]
pub(super) struct AvatarCatalog(BTreeMap<(u8, u8), Body>);
#[derive(Clone)]
struct Body {
    display: u32,
    faction: u32,
    classes: Vec<u8>,
    ranges: [u8; 5],
    hair_colors: Vec<u8>,
}
impl AvatarCatalog {
    pub(super) fn from_client(catalog: &benilla_formats::CharCreateCatalog) -> Self {
        let mut bodies = BTreeMap::new();
        for race in catalog
            .race_rows()
            .iter()
            .filter(|r| (1..=8).contains(&r.id))
        {
            for gender in [0, 1] {
                let (Some(display), Some(ranges)) = (
                    catalog.body_display(race.id, gender),
                    catalog.ranges(race.id, gender),
                ) else {
                    continue;
                };
                bodies.insert(
                    (race.id, gender),
                    Body {
                        display,
                        faction: race.faction,
                        classes: catalog.classes_for_race(race.id),
                        ranges: [
                            ranges.skin,
                            ranges.face,
                            ranges.hair_style,
                            ranges.hair_color,
                            ranges.facial_hair,
                        ],
                        hair_colors: (0..ranges.hair_style)
                            .map(|style| catalog.hair_colors(race.id, gender, style))
                            .collect(),
                    },
                );
            }
        }
        Self(bodies)
    }
    pub(super) fn validate(&self, race: u8, class: u8, gender: u8, look: Appearance) -> Result<()> {
        let body = self
            .0
            .get(&(race, gender))
            .context("character body is unavailable in this client install")?;
        let values = [
            look.skin,
            look.face,
            look.hair_style,
            look.hair_color,
            look.facial_hair,
        ];
        if !body.classes.contains(&class)
            || values
                .iter()
                .zip(body.ranges)
                .any(|(&value, count)| value >= count.max(1))
            || look.hair_color
                >= body
                    .hair_colors
                    .get(usize::from(look.hair_style))
                    .copied()
                    .unwrap_or(0)
                    .max(1)
        {
            bail!("character appearance is unavailable in this client install");
        }
        Ok(())
    }
    pub(super) fn body(&self, character: &Character) -> Result<(u32, u32)> {
        self.validate(
            character.race,
            character.class,
            character.gender,
            character.appearance,
        )?;
        let body = &self.0[&(character.race, character.gender)];
        Ok((body.display, body.faction))
    }
    #[cfg(test)]
    pub(super) fn fixture() -> Self {
        Self(
            [
                (
                    (1, 0),
                    Body {
                        display: 49,
                        faction: 1,
                        classes: vec![1, 2, 4, 5, 8, 9],
                        ranges: [4; 5],
                        hair_colors: vec![4; 4],
                    },
                ),
                (
                    (2, 1),
                    Body {
                        display: 52,
                        faction: 2,
                        classes: vec![1, 3, 4, 7, 9],
                        ranges: [4; 5],
                        hair_colors: vec![4; 4],
                    },
                ),
            ]
            .into(),
        )
    }
}

pub(super) fn roster_character(c: &Character) -> RosterCharacter {
    RosterCharacter {
        guid: character_guid(c),
        name: c.name.clone(),
        race: c.race,
        class: c.class,
        gender: c.gender,
        skin: c.appearance.skin,
        face: c.appearance.face,
        hair_style: c.appearance.hair_style,
        hair_color: c.appearance.hair_color,
        facial_hair: c.appearance.facial_hair,
        level: c.level,
        zone: c.position.zone_id,
        map: c.position.map_id,
        position: benilla_protocol::wire::Vector3d {
            x: c.position.position[0],
            y: c.position.position[1],
            z: c.position.position[2],
        },
        flags: 0,
        equipment: [benilla_protocol::messages::CharEnumItem::default(); 19],
        pet_display_id: 0,
        pet_level: 0,
        pet_family: 0,
    }
}
fn realm(count: usize) -> benilla_protocol::RealmInfo {
    benilla_protocol::RealmInfo {
        name: "Local P2P characters".into(),
        address: String::new(),
        population: 0.0,
        characters: count as u8,
        realm_type: 0,
        flags: 0,
        category: 0,
        id: 0,
    }
}
fn roster(
    store: &ProfileStore,
    events: &Sender<SessionEvent>,
    cause: RosterCause,
    generation: u64,
) -> Result<()> {
    let characters: Vec<_> = store.characters().iter().map(roster_character).collect();
    let unique: std::collections::BTreeSet<_> = characters.iter().map(|c| c.guid).collect();
    if unique.len() != characters.len() {
        bail!("local character GUID collision");
    }
    emit(
        events,
        SessionEvent::CharacterList {
            realm: Some(realm(characters.len())),
            characters,
            cause,
            epoch: 1,
            generation,
        },
    )
}
fn create_input(req: benilla_protocol::CharCreateReq) -> NewCharacter {
    NewCharacter {
        name: req.name,
        race: req.race,
        class: req.class,
        gender: req.gender,
        appearance: Appearance {
            skin: req.skin,
            face: req.face,
            hair_style: req.hair_style,
            hair_color: req.hair_color,
            facial_hair: req.facial_hair,
        },
    }
}

/// Opening never replaces an unreadable profile; creation happens only at this deliberate UI edge.
pub(super) fn browse(
    settings: P2pSettings,
    root: &Path,
    catalog: &AvatarCatalog,
    generation: u64,
    events: &Sender<SessionEvent>,
    commands: &Receiver<ClientCommand>,
    park: &Receiver<ParkRequest>,
    ticket: &P2pEndpointTicket,
    stop: &AtomicBool,
    local_zone: &AtomicU32,
) -> Result<()> {
    let mut store = if root.try_exists()? {
        ProfileStore::open(root)?
    } else {
        std::fs::create_dir_all(root.parent().context("profile directory has no parent")?)?;
        ProfileStore::create(root)?
    };
    for character in store.characters() {
        catalog.body(character)?;
    }
    emit(
        events,
        SessionEvent::WorldAuthenticated {
            realm: realm(store.characters().len()),
            epoch: 1,
            generation,
        },
    )?;
    roster(&store, events, RosterCause::Park, generation)?;
    loop {
        if stop.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        match park.recv_timeout(Duration::from_millis(20)) {
            Ok(ParkRequest::Char(request)) => match request {
                super::super::CharRequest::Enumerate => {
                    roster(&store, events, RosterCause::Asked, generation)?
                }
                super::super::CharRequest::Create(req) => {
                    let input = create_input(req);
                    let code = if input.validate().is_err()
                        || catalog
                            .validate(input.race, input.class, input.gender, input.appearance)
                            .is_err()
                    {
                        0x30 // CHAR_CREATE_FAILED; no write or network action occurred.
                    } else if store
                        .characters()
                        .iter()
                        .any(|c| c.name.eq_ignore_ascii_case(&input.name))
                    {
                        benilla_protocol::messages::CHAR_CREATE_NAME_IN_USE
                    } else if store.characters().len() >= benilla_profile::MAX_CHARACTERS {
                        benilla_protocol::messages::CHAR_CREATE_SERVER_LIMIT
                    } else {
                        store
                            .create_character(input)
                            .context("could not save character; local profile closed")?;
                        roster(&store, events, RosterCause::Changed, generation)?;
                        benilla_protocol::messages::CHAR_CREATE_SUCCESS
                    };
                    emit(
                        events,
                        SessionEvent::CharActionResult {
                            action: CharAction::Create,
                            code,
                        },
                    )?;
                }
                super::super::CharRequest::Enter(wanted) => {
                    let Some(mut character) = store
                        .characters()
                        .iter()
                        .find(|c| character_guid(c) == wanted)
                        .cloned()
                    else {
                        emit(events, SessionEvent::CharacterLoginFailed { result: 5 })?;
                        continue;
                    };
                    catalog.body(&character)?;
                    store
                        .select_character(character.id)
                        .context("could not save character selection; local profile closed")?;
                    // Stale world requests cannot move or log out the next character.
                    while commands.try_recv().is_ok() {}
                    let result = run_character(
                        settings.clone(),
                        events,
                        commands,
                        ticket,
                        &mut store,
                        &mut character,
                        catalog,
                        stop,
                        local_zone,
                    );
                    // All exits (logout, transport loss, app close) checkpoint only our local record.
                    store.save_position(character.id, character.position).context("could not save local position; session stopped, check local profile before rejoining")?;
                    result?;
                    ticket.clear();
                    if stop.load(std::sync::atomic::Ordering::SeqCst) {
                        return Ok(());
                    }
                    emit(events, SessionEvent::LoggedOut)?;
                    emit(
                        events,
                        SessionEvent::Disconnected {
                            reason: "P2P character saved".into(),
                            end: SessionEnd::LoggedOut,
                        },
                    )?;
                    roster(&store, events, RosterCause::Park, generation)?;
                }
                super::super::CharRequest::Abandon => return Ok(()),
                // Deliberately unsupported until recoverable deletion/rename semantics are defined.
                super::super::CharRequest::Delete(_) => emit(
                    events,
                    SessionEvent::CharActionResult {
                        action: CharAction::Delete,
                        code: 0x3a,
                    },
                )?,
                super::super::CharRequest::Rename { .. } => emit(
                    events,
                    SessionEvent::CharRenamed {
                        result: 1,
                        renamed: None,
                    },
                )?,
            },
            Ok(ParkRequest::Realm(super::super::RealmRequest::Refresh)) => emit(
                events,
                SessionEvent::RealmList {
                    realms: Vec::new(),
                    failed: true,
                },
            )?,
            Ok(ParkRequest::Realm(super::super::RealmRequest::Close)) => {}
            Ok(ParkRequest::Realm(_)) => return Ok(()),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}
