// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::player::{
    ItemLifetimeObservation, PlayerCreateRows, SourcePlayer, SourcePlayerAuraClock,
    SourcePlayerCastInputs, SourcePlayerCreateConfig, SourcePlayerCreation, SourcePlayerItemInputs,
    SourcePlayerSaveError, SourcePlayerSaveInputs, SourcePlayerSaveStore,
};
use super::*;
use benilla_protocol::messages::{opcode::SMSG_CHAR_CREATE, CHAR_CREATE_SUCCESS};
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};

const CHAR_CREATE_ERROR: u8 = 0x2f; // SharedDefines.h:1799.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCharacterCreateObservation {
    ResetRosterCount { account: u32 },
    RosterEntry { account: u32, guid: u32 },
    Packet { opcode: u16, result: u8 },
}

#[derive(Debug)]
pub enum SourceCharacterCreateError<E> {
    Source(Refusal),
    Save(SourcePlayerSaveError<E>),
    Discard {
        error: Refusal,
        previous: Option<Box<Self>>,
        receipt_confirmed: bool,
    },
}

pub struct SourceCharacterCreateCast<'a> {
    pub targets: &'a SourceSpellTargetMgr,
    pub terrain: &'a mut SourceTerrainManager,
    pub map_time: &'a dyn Fn(MapIdentity) -> Result<u32>,
    pub aura_seconds: &'a mut dyn FnMut(SourcePlayerAuraClock) -> Result<i64>,
    pub max_casts: u32,
    pub indoor_policy: bool,
}

pub struct SourceCharacterCreateInputs<'a, W> {
    pub save: SourcePlayerSaveInputs<'a, W>,
    pub cells: &'a mut SourceCells,
    pub item_events: &'a mut dyn FnMut(ItemLifetimeObservation),
    pub guids: &'a mut GlobalGuidAllocators,
    pub player_rows: &'a PlayerCreateRows,
    pub item_templates: &'a SourceStartupItems,
    pub models: &'a LoadedCreatureModels,
    pub config: SourcePlayerCreateConfig,
    pub cast: SourceCharacterCreateCast<'a>,
    pub thread: u64,
    pub have_same_race: bool,
    pub skip_cinematics: u32,
}

/// CharacterHandler.cpp:494-525 after request admission. Allocate the low GUID here.
pub fn create_character_after_preflight<S: SourcePlayerSaveStore>(
    mut request: SourcePlayerCreation,
    inputs: &mut SourceCharacterCreateInputs<'_, impl WorldStartupWork>,
    random: &mut SourceRandom<'_, impl RawGenerator, impl FnMut(TraceEvent)>,
    store: &mut S,
    mut observe: impl FnMut(SourceCharacterCreateObservation),
) -> Result<S::Receipt, SourceCharacterCreateError<S::Error>> {
    use SourceCharacterCreateError::{Discard, Save, Source};

    store
        .check_open()
        .map_err(|error| Save(SourcePlayerSaveError::Storage(error)))?;
    let live = &mut inputs.save;
    let mut player = SourcePlayer::construct(
        live.objects,
        live.units,
        inputs.thread,
        inputs.config.save_interval,
        |min, max| {
            random
                .urand(min, max)
                .map_err(|_| refuse("execution.character_login.Player_515.random_owner"))
        },
        &mut live.wall_seconds,
    )
    .map_err(Source)?;

    let mut receipt_confirmed = false;
    let outcome = (|| {
        // CharacterHandler.cpp:495; ObjectMgr.h:817.
        let guid = inputs.guids.allocate(HighGuid::Player).map_err(Source)?;
        require(
            !guid.stop_now,
            "execution.character_login.CharacterHandler_495.player_guid_stop_owner",
        )
        .map_err(Source)?;
        request.guid_low = guid.counter;
        player
            .create(
                live.objects,
                live.units,
                request,
                inputs.player_rows,
                live.spells,
                inputs.item_templates,
                inputs.models,
                inputs.cells,
                inputs.config,
                Some(SourcePlayerCastInputs {
                    targets: inputs.cast.targets,
                    terrain: inputs.cast.terrain,
                    map_time: inputs.cast.map_time,
                    aura_seconds: inputs.cast.aura_seconds,
                    irand: &mut |min, max| {
                        random.irand(min, max).map_err(|_| {
                            refuse("execution.character_login.Object_2957.random_owner")
                        })
                    },
                    session: live.session,
                    max_casts: inputs.cast.max_casts,
                    indoor_policy: inputs.cast.indoor_policy,
                }),
                Some(SourcePlayerItemInputs {
                    items: live.items,
                    guids: inputs.guids,
                    observe: inputs.item_events,
                    logging_out: false, // CharacterHandler.cpp:494. The new Player is detached.
                }),
                &mut live.wall_seconds,
            )
            .map_err(Source)?;

        player.set_creation_flags(inputs.have_same_race, inputs.skip_cinematics);
        // CharacterHandler.cpp:512. The admitted Create has stats saving disabled and no pet.
        let (receipt, guid) = player.save_created_to_db(live, store).map_err(Save)?;
        receipt_confirmed = true;
        let account = live.session.account_id();
        // Deviation: CharacterHandler.cpp:515-516 observes the single committed roster entry
        // because the solo world has no LoginDatabase.
        observe(SourceCharacterCreateObservation::ResetRosterCount { account });
        observe(SourceCharacterCreateObservation::RosterEntry { account, guid });
        send_result(live.session, CHAR_CREATE_SUCCESS, &mut observe).map_err(Source)?;
        Ok(receipt)
    })();

    // CharacterHandler.cpp:498,525 destroys the detached Player on both paths.
    if let Err(error) = player.discard(
        live.objects,
        live.units,
        inputs.cells,
        live.items,
        inputs.item_events,
    ) {
        return Err(Discard {
            error,
            previous: outcome.err().map(Box::new),
            receipt_confirmed,
        });
    }
    if !receipt_confirmed && matches!(&outcome, Err(Source(_))) {
        send_result(live.session, CHAR_CREATE_ERROR, &mut observe).map_err(Source)?;
    }
    outcome
}

fn send_result(
    session: &SourceWorldSession,
    result: u8,
    observe: &mut impl FnMut(SourceCharacterCreateObservation),
) -> Result<()> {
    // CharacterHandler.cpp:501,519 forces delivery during character selection.
    session.send_packet(SMSG_CHAR_CREATE, &[result], true, |opcode, _| {
        observe(SourceCharacterCreateObservation::Packet { opcode, result });
        Ok(())
    })
}
