// SPDX-License-Identifier: GPL-3.0-or-later
//! Frozen diagnostic clock/calendar inputs. Manager outcomes are computed, never injected.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_sim::{source_calendar::Tm, source_moon as moon};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[path = "seal16_active_init.rs"]
mod active_init;
#[path = "seal16_creature_models.rs"]
mod creature_models;
#[path = "seal16_equipment.rs"]
mod equipment;
#[path = "seal16_formation_path.rs"]
mod formation_path;
#[path = "seal16_gameobject_models.rs"]
mod gameobject_models;
#[path = "seal16_immunities.rs"]
mod immunities;
#[path = "seal16_relations_evidence.rs"]
mod relations_evidence;
#[path = "seal16_script_registry.rs"]
mod script_registry;
#[path = "seal16_spawn_data.rs"]
mod spawn_data;
#[path = "seal16_spawn_init.rs"]
mod spawn_init;
#[path = "seal16_spell_list_reach.rs"]
mod spell_list_reach;
#[path = "seal16_spell_lists.rs"]
mod spell_lists;
#[path = "seal16_templates.rs"]
pub(super) mod templates;
#[path = "seal16_waypoints.rs"]
mod waypoints;

const GROUP_QUERIES: [&str; 6] = [
    "SELECT Id, Name, Type, MaxCount, WorldState, WorldStateExpression, Flags, StringId, RespawnOverrideMin, RespawnOverrideMax FROM spawn_group",
    "SELECT Id, FormationType, FormationSpread, FormationOptions, PathId, MovementType, Comment FROM spawn_group_formation",
    "SELECT Id, Guid, SlotId, Chance FROM spawn_group_spawn",
    "SELECT Id, Entry, MinCount, MaxCount, Chance FROM spawn_group_entry",
    "SELECT Id, LinkedId FROM spawn_group_linked_group",
    "SELECT Id, SquadId, Guid, Entry FROM spawn_group_squad",
];
const EVENT_AI_QUERY: &str = "SELECT id, creature_id, event_type, event_inverse_phase_mask, event_chance, event_flags, event_param1, event_param2, event_param3, event_param4, event_param5, event_param6, action1_type, action1_param1, action1_param2, action1_param3, action2_type, action2_param1, action2_param2, action2_param3, action3_type, action3_param1, action3_param2, action3_param3 FROM creature_ai_scripts ORDER BY id";
const RANDOM_TEMPLATE_QUERY: &str =
    "SELECT id, type, target_id, chance FROM dbscript_random_templates";
// The first word of npc_chicken_cluck's zero-action CreatureAI base.
const MAP0_CHICKEN_TIMER: u64 = 60580494;
const MAP0_CHICKEN_RAW: usize = 27356;
const MAP0_GUARD_RAW: usize = 27328;
const MAP0_BELL_RAW: usize = 27472; // First word after the factory, :60638177.
const MAP0_EVENT_RAW: usize = 27682; // Event 26501 holder timer word, :60711960.
const MAP0_INVISIBILITY_RAW: usize = 27780; // First word after aura 18, :60774115.
const MAP0_NEXT_RAW: usize = 28714; // First unconsumed word, :60985315.

// Aura 79 runs after its holder's three effects are created and before this success row.
const MAP0_NEXT_OWNER_LINE: u64 = 60985257;

fn retained_f1_file(root: &Path, recorded: &Path, expected: &Sha256Id) -> Result<Vec<u8>> {
    let prefix = recorded
        .ancestors()
        .find(|path| path.ends_with("benilla-config/full-server/f1"))
        .ok_or_else(|| refuse("execution.startup.recorded_f1_path_component_missing"))?;
    let suffix = recorded.strip_prefix(prefix).unwrap();
    require(
        !suffix.as_os_str().is_empty()
            && suffix
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
        "execution.startup.recorded_f1_path_suffix",
    )?;
    let bytes = std::fs::read(root.join(suffix))
        .map_err(|_| refuse("execution.startup.recorded_f1_file_missing"))?;
    require(
        Sha256Id::of(&bytes) == *expected,
        "execution.startup.recorded_f1_hash_mismatch",
    )?;
    Ok(bytes)
}

fn map0_receipt<const N: usize>(rows: impl Iterator<Item = [u64; N]>) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    for row in rows {
        for word in row {
            digest.update(word.to_le_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

fn assert_map0_continuation(evidence: &MapThreadEvidence, clocks: &active_init::ActiveClocks) {
    // The GUID40520 birth was already counted. The remaining 53 births include
    // the trap at :60493861 and both duplicate creature attempts.
    // The continuation through the Bell GO adds 51 births, :60580681-60637267.
    assert_eq!(evidence.continuation_births.len(), 363);
    let births = &evidence.continuation_births[1..54];
    assert_eq!((births[0], births[52]), (60478620, 60571587));
    assert_eq!(
        map0_receipt(births.iter().map(|&line| [line])),
        "fd02f4a98005a10bdacc5c1d342b14d97a8c9ac57409e744915deef010045e54"
    );
    assert_eq!(
        &evidence.continuation_births[54..60],
        [60574126, 60575661, 60576274, 60576775, 60577108, 60579285]
    );
    assert_eq!(
        &evidence.continuation_completed[..56],
        [
            (60478609, 40520),
            (60479696, 1578),
            (60480468, 2214),
            (60481282, 1691),
            (60482150, 832),
            (60483239, 792),
            (60484244, 776),
            (60485215, 2505),
            (60486252, 914),
            (60487151, 1644),
            (60488201, 774),
            (60493566, 2129),
            (60494135, 33),
            (60497062, 1396),
            (60497827, 1398),
            (60505039, 1626),
            (60507532, 2243),
            (60508282, 1393),
            (60509032, 1397),
            (60509997, 1399),
            (60510905, 534),
            (60511807, 1182),
            (60512087, 191740),
            (60514135, 5071),
            (60514574, 73947),
            (60514960, 74172),
            (60516665, 5072),
            (60517151, 4232),
            (60546056, 4344),
            (60547268, 4415),
            (60548637, 4384),
            (60550089, 4936),
            (60550653, 15035),
            (60551079, 15037),
            (60551556, 191547),
            (60552874, 4416),
            (60554206, 4245),
            (60554658, 15023),
            (60555085, 15028),
            (60555778, 14801),
            (60556251, 14802),
            (60556840, 14807),
            (60557436, 14810),
            (60558109, 14965),
            (60558865, 14803),
            (60559535, 14914),
            (60566593, 5941),
            (60567087, 14068),
            (60567881, 14798),
            (60571579, 4086),
            (60574113, 4242),
            (60575629, 4244),
            (60576242, 5015),
            (60576763, 15029),
            (60577106, 15033),
            (60579270, 4198),
        ]
    );
    let births = &evidence.continuation_births[60..111];
    assert_eq!((births[0], births[50]), (60580681, 60637267));
    assert_eq!(
        map0_receipt(births.iter().map(|&line| [line])),
        "6234e9fada61f0581a42108b6276df7ec12bd3a9835545448d6e921930eee51a"
    );
    let completions = &evidence.continuation_completed[56..102];
    assert_eq!(completions.len(), 46);
    assert_eq!(completions[0], (60580671, 4228));
    assert_eq!(completions[45], (60637264, 15421));
    assert_eq!(
        map0_receipt(
            completions
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "ae02fead37484a452953c8deba64e4f343ee472f6908803227b71f52bbf2cdd8"
    );
    // GO15508 completes first. The last allocation is GUID4195.
    let births = &evidence.continuation_births[111..180];
    assert_eq!((births[0], births[68]), (60638125, 60709759));
    assert_eq!(
        map0_receipt(births.iter().map(|&line| [line])),
        "4fb63486dea7a44326a4a25eb55d3900e5da2dd7f2b3da658f290c52847d7a57"
    );
    let completions = &evidence.continuation_completed[102..166];
    assert_eq!(completions[0], (60638112, 15508));
    assert_eq!(completions[63], (60709753, 4083));
    assert_eq!(
        map0_receipt(
            completions
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "cc1c77e7c09ff82f9453a9f4274125ff29427d258eae5a0488ed60669b564f77"
    );
    let births = &evidence.continuation_births[180..223];
    assert_eq!((births[0], births[42]), (60712001, 60773214));
    assert_eq!(
        map0_receipt(births.iter().map(|&line| [line])),
        "d296165794fc52b17bcedae926207208f9ea132758b118fea556c3520e6fefa1"
    );
    let completions = &evidence.continuation_completed[166..203];
    assert_eq!(completions.len(), 37);
    assert_eq!(completions[0], (60711999, 4195));
    assert_eq!(completions[36], (60773166, 4192));
    assert_eq!(
        map0_receipt(
            completions
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "a8004cf6b20c65621eb50b1e4d116cc60e05f2283856c7d030b1cdf33b26611b"
    );
    // GUID151461 was already allocated. Count all later births through GUID12058.
    let births = &evidence.continuation_births[223..];
    assert_eq!(
        (births.len(), births[0], births[139]),
        (140, 60774483, 60982852)
    );
    assert_eq!(
        map0_receipt(births.iter().map(|&line| [line])),
        "20cf893a5fa80a648599e9b040ed0dc3e0fe025bfa2dfe7cdb5e021afe487728"
    );
    let completions = &evidence.continuation_completed[203..];
    assert_eq!(completions.len(), 139);
    assert_eq!(completions[0], (60774468, 151461));
    assert_eq!(completions[138], (60982845, 12056));
    assert_eq!(
        map0_receipt(
            completions
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "8cd3ad671bc0134f0bbd1cf45247d55f265ad1eb61bb9675f1c8066071483632"
    );
    // The replay also compares each word, request and result in order below.
    let words = &evidence.words[27087..MAP0_GUARD_RAW];
    assert_eq!(words[0], (60478655, 27087, 2231193506));
    assert_eq!(words[240], (60572458, 27327, 259592748));
    assert_eq!(
        map0_receipt(words.iter().map(|&(line, raw, word)| [line, raw, word])),
        "81f7cbfb2bffbf5cd21af1a54cbde3b1aaf37622fbdb49567601059a266f4353"
    );
    // The first pair is Timer::Timer (:23); the second is ResetTimer (:47).
    // SpellListChanged has no draw at :60574013. JustRespawned starts at :60574032.
    assert_eq!(
        &evidence.words[MAP0_GUARD_RAW..MAP0_CHICKEN_RAW],
        [
            (60573960, 27328, 4005247489),
            (60573993, 27329, 1795029007),
            (60574042, 27330, 2754521498),
            (60574062, 27331, 3095061613),
            (60574172, 27332, 3268678362),
            (60574613, 27333, 704630599),
            (60575486, 27334, 1562681081),
            (60575508, 27335, 1179579994),
            (60575569, 27336, 3428138308),
            (60575587, 27337, 3725666063),
            (60575719, 27338, 2615642661),
            (60576072, 27339, 978124531),
            (60576125, 27340, 3028640038),
            (60576135, 27341, 1684947418),
            (60576160, 27342, 1374585815),
            (60576170, 27343, 3439775274),
            (60576185, 27344, 1558588162),
            (60576202, 27345, 3443523208),
            (60576715, 27346, 292624312),
            (60577087, 27347, 589746404),
            (60577126, 27348, 528491939),
            (60577863, 27349, 3249244699),
            (60579113, 27350, 3022591220),
            (60579136, 27351, 3061391942),
            (60579199, 27352, 1570955401),
            (60579219, 27353, 4225726343),
            (60579340, 27354, 2157508273),
            (60579530, 27355, 1618006803),
        ]
    );
    assert_eq!(
        &evidence.words[MAP0_CHICKEN_RAW..MAP0_CHICKEN_RAW + 4],
        [
            // TimerAI.cpp:23,47. The SpellListChanged dispatch at :60580583 draws nothing.
            (MAP0_CHICKEN_TIMER, 27356, 360747331),
            (60580531, 27357, 2445076291),
            (60580617, 27358, 1612564339),
            (60580637, 27359, 843612412),
        ]
    );
    // 116 new words, :60580494-60637549, including the four chicken timer draws.
    let words = &evidence.words[MAP0_CHICKEN_RAW..MAP0_BELL_RAW];
    assert_eq!(words[115], (60637549, 27471, 2490111375));
    assert_eq!(
        map0_receipt(words.iter().map(|&(line, raw, word)| [line, raw, word])),
        "5c0d7a8a781036bde0b46a2c740a259b055e7da8b544a08bca2b0fdb182ca404"
    );
    assert_eq!(evidence.words[MAP0_BELL_RAW], (60638177, 27472, 1912213781));
    assert_eq!(
        evidence.words[MAP0_EVENT_RAW],
        (60711960, 27682, 3955263560)
    );
    assert_eq!(
        map0_receipt(
            evidence.words[MAP0_BELL_RAW..MAP0_EVENT_RAW]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "001109888bc6c245941e9b4a64f1d9f7069f083089cb5ecf0e36d713dd45413f"
    );
    assert_eq!(
        evidence.words[MAP0_INVISIBILITY_RAW],
        (60774115, 27780, 2990556059)
    );
    assert_eq!(
        map0_receipt(
            evidence.words[MAP0_EVENT_RAW..MAP0_INVISIBILITY_RAW]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "96bc1ebfc5ffb560ee541af7452d722145260e776bcbafa10de49116d67bf2ca"
    );
    assert_eq!(
        evidence.words[MAP0_NEXT_RAW - 1],
        (60984363, 28713, 3627799220)
    );
    assert_eq!(evidence.words[MAP0_NEXT_RAW], (60985315, 28714, 3265115472));
    assert_eq!(
        map0_receipt(
            evidence.words[MAP0_INVISIBILITY_RAW..MAP0_NEXT_RAW]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "e42db86f0d75705ba1476da10c11b2132caebfe345ef3703d8f7247ce8b6f79a"
    );
    let consumed = evidence.active_clocks.0.len() - clocks.0.len();
    let new_clocks = evidence
        .active_clocks
        .0
        .iter()
        .take(consumed)
        .filter(|clock| clock.time_line > 60478161);
    assert_eq!(consumed, 303);
    assert_eq!(new_clocks.clone().count(), 225);
    assert_eq!(
        map0_receipt(new_clocks.clone().take(34).map(|clock| [
            clock.site_line,
            clock.time_line,
            clock.thread,
            clock.site[2],
            clock.raw,
            clock.seconds as u64
        ])),
        "a6728ca86005a2928aaa1272041a9130b0ec2a2a5541b5abb0e3d28041cd3630"
    );
    assert_eq!(
        new_clocks
            .clone()
            .skip(34)
            .take(4)
            .map(|clock| (clock.site_line, clock.time_line, clock.raw, clock.seconds))
            .collect::<Vec<_>>(),
        [
            (60574816, 60574819, 27334, 1794314190),
            (60576112, 60576113, 27340, 1794314190),
            (60578080, 60578083, 27350, 1794314190),
            (60579682, 60579683, 27356, 1794314190),
        ]
    );
    assert_eq!(
        new_clocks
            .clone()
            .skip(38)
            .take(13)
            .map(|clock| (clock.site_line, clock.time_line, clock.raw, clock.seconds))
            .collect::<Vec<_>>(),
        [
            (60581518, 60581521, 27362, 1794314190),
            (60585022, 60585025, 27372, 1794314190),
            (60587353, 60587356, 27378, 1794314190),
            (60597330, 60597331, 27394, 1794314190),
            (60598552, 60598555, 27400, 1794314190),
            (60600578, 60600579, 27406, 1794314190),
            (60605634, 60605636, 27412, 1794314190),
            (60607322, 60607335, 27418, 1794314190),
            (60608605, 60608606, 27424, 1794314190),
            (60611355, 60611358, 27430, 1794314190),
            (60616070, 60616071, 27440, 1794314190),
            (60624558, 60624560, 27446, 1794314190),
            (60625671, 60625673, 27452, 1794314190),
        ]
    );
    assert_eq!(
        map0_receipt(new_clocks.clone().skip(51).take(28).map(|clock| [
            clock.site_line,
            clock.time_line,
            clock.thread,
            clock.site[2],
            clock.raw,
            clock.seconds as u64
        ])),
        "63f782289ea081e8c04b7c1f2eef65026c343c88ba8181e5818110fce1862f2e"
    );
    assert_eq!(
        map0_receipt(new_clocks.clone().skip(79).take(13).map(|clock| [
            clock.site_line,
            clock.time_line,
            clock.thread,
            clock.site[2],
            clock.raw,
            clock.seconds as u64
        ])),
        "209fa20e3fa36eea564c69dd6ffac48818a98b2c1c5c879a963cf78ff2a0ad81"
    );
    // Clock :60774092 precedes the old refusal. The next 133 include both stances.
    assert_eq!(evidence.active_clocks.0[169].time_line, 60774092);
    assert_eq!(evidence.active_clocks.0[170].time_line, 60774274);
    assert_eq!(
        map0_receipt(new_clocks.skip(92).map(|clock| [
            clock.site_line,
            clock.time_line,
            clock.thread,
            clock.site[2],
            clock.raw,
            clock.seconds as u64
        ])),
        "6b59dfe0728a88a09b67bc9f03123b879a6414c7f2c53f083c1bb95f962dee45"
    );
    assert_eq!(evidence.active_clocks.0[consumed - 1].time_line, 60985213);
    assert_eq!(clocks.0.front().unwrap().time_line, 60985940);
}

fn assert_guid40520_queue(spells: &SourceSpellStarts, events: &SourceUnitEvents) {
    let queue = events.inventory();
    assert_eq!(queue.clock_ms, 0);
    assert!(queue.detached.is_empty());
    assert_eq!(queue.queued.len(), 5);
    // Spell.cpp:3007-3009. Trace :60477109, :60477323, :60477770, :60478176.
    for (id, event) in (1..=4).zip(&queue.queued) {
        assert_eq!(event.kind, SourceUnitEvent::Spell { spell: id });
        assert_eq!(spells.spells[&id].event, Some(event.event_id));
        assert_eq!(
            (event.add_time_ms, event.exec_time_ms, event.to_abort),
            (0, 1, false)
        );
    }
    // Creature::LoadFromDB schedules the later AI notification in :60477593.
    assert_eq!(queue.queued[4].kind, SourceUnitEvent::AiNotify);
    assert_eq!(queue.queued[4].exec_time_ms, 5000);
}

#[test]
fn retained_f1_snapshot_preserves_nested_suffix_after_relocation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(root.join("src/lib.rs")).unwrap();
    assert_eq!(
        retained_f1_file(
            root,
            Path::new("/old/benilla-config/full-server/f1/src/lib.rs"),
            &Sha256Id::of(&bytes),
        )
        .unwrap(),
        bytes
    );
}

#[test]
fn retained_f1_snapshot_refuses_missing_component_and_hash_mismatch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let wrong = Sha256Id::of(b"not the retained snapshot");
    assert_eq!(
        retained_f1_file(root, Path::new("/old/src/lib.rs"), &wrong)
            .unwrap_err()
            .id,
        "execution.startup.recorded_f1_path_component_missing"
    );
    assert_eq!(
        retained_f1_file(
            root,
            Path::new("/old/benilla-config/full-server/f1/src/lib.rs"),
            &wrong,
        )
        .unwrap_err()
        .id,
        "execution.startup.recorded_f1_hash_mismatch"
    );
    assert_eq!(
        retained_f1_file(
            root,
            Path::new("/old/benilla-config/full-server/f1/../src/lib.rs"),
            &wrong,
        )
        .unwrap_err()
        .id,
        "execution.startup.recorded_f1_path_suffix"
    );
}

#[derive(Clone, Debug)]
enum CalendarRecord {
    Now(i64),
    Local(i64, Tm),
    Epoch([i32; 7], i64, Tm),
}
fn tm(p: &[&str]) -> Tm {
    let n = |i: usize| p[i].parse().unwrap();
    Tm {
        sec: n(0),
        min: n(1),
        hour: n(2),
        mday: n(3),
        mon: n(4),
        year: n(5),
        wday: n(6),
        yday: n(7),
        isdst: n(8),
    }
}
// ScriptDevAIMgr.cpp:370 dispatches npc_solenor at trace :60657539.
const MAP1_NEXT_OWNER_LINE: u64 = 60657539;

#[derive(Default)]
struct MapThreadEvidence {
    initial: Option<benilla_sim::source_rng::MtState>,
    first_word: Option<(u64, u64, u64)>,
    before_sql: Vec<(u64, u64, u64)>,
    words: Vec<(u64, u64, u64)>,
    sql_line: Option<u64>,
    sql_statements: Vec<Vec<u8>>,
    sql_lines: Vec<u64>,
    first_shuffle: Option<String>,
    shuffle_before: Vec<u32>,
    shuffle_after: Vec<u32>,
    total_words: usize,
    pool_raw_end: Option<usize>,
    pool_snapshot_line: Option<u64>,
    total_shuffles: usize,
    pool_shuffles: Option<usize>,
    pool_members: [BTreeSet<u32>; 2],
    pool_counts: BTreeMap<u32, u32>,
    spawn_clock_site: bool,
    spawn_clock: Option<(u64, i64, usize)>,
    active_clock_site: Option<(u64, [u64; 3], bool)>,
    active_clocks: active_init::ActiveClocks,
    collecting_spawn_groups: bool,
    spawn_groups: BTreeSet<u32>,
    first_grid: Option<(u64, u16, u16)>,
    first_candidate: Option<(u64, u32, u32, u16, u16)>,
    first_mmap_query: Option<(u64, u32)>,
    first_candidate_rng: Vec<(String, u32, i64, i64, Option<i64>)>,
    candidate_requests: Vec<(usize, String, i64, i64, Option<i64>)>,
    first_created_guid: Option<(u64, u32, u32, u32, u16)>,
    active_adds: Vec<(u64, u32, String, u32)>,
    active_locks: Vec<(u64, u16)>,
    continuation_births: Vec<u64>,
    continuation_completed: Vec<(u64, u32)>,
    path_calculations: Vec<u64>,
    wmo_area_hit: Option<String>,
    bell_trace: Vec<u64>,
}
#[derive(Default)]
pub(super) struct Inputs {
    query: Vec<u8>,
    pointer: String,
    event: u16,
    clock_site: u64,
    query_operation: u64,
    query_occurrence: u64,
    load_clocks: VecDeque<(EventClockSite, ClockSample)>,
    update_clocks: VecDeque<i64>,
    check_clocks: VecDeque<i64>,
    calendar: VecDeque<CalendarRecord>,
    epoch: Option<[i32; 7]>,
    world_seconds: Option<i64>,
    persistent_owners: [Option<InputOwner>; 2],
    updating: bool,
    expected_sql: Vec<String>,
    map_births: usize,
    persistent_creations: Vec<(u32, u32)>,
    map_identities: BTreeMap<u64, (u64, String)>,
    map_bindings: BTreeMap<u32, (u64, u64, u64)>,
    map_threads: BTreeMap<u64, MapThreadEvidence>,
    event_initialize_line: Option<u64>,
    event_status_writes: Vec<(u64, String)>,
    graveyard_query_line: Option<u64>,
    graveyard_rows: Vec<[u32; 4]>,
    graveyard_last_field_line: Option<u64>,
    world_raw_words: u64,
    graveyard_raw_start: Option<u64>,
    graveyard_raw_end: Option<u64>,
    encounter_sql: BTreeMap<String, Vec<Vec<Vec<u8>>>>,
    encounter_queries: Vec<(u64, String)>,
    encounter_raw: BTreeMap<String, (u64, u64)>,
    group_queries: Vec<(u64, String)>,
    waypoint_queries: Vec<(u64, String)>,
    corpse_query_line: Option<u64>,
    templates: templates::Evidence,
    spawn_data: spawn_data::Evidence,
    creature_models: creature_models::Evidence,
    equipment: equipment::Evidence,
    spell_lists: spell_lists::Evidence,
    immunities: immunities::Evidence,
    script_registry: script_registry::Evidence,
    loaded_spawn_templates: Option<LoadedCreatureSpawnTemplates>,
    random_template_request_line: Option<u64>,
    random_template_rows: Vec<[u32; 4]>,
    event_ai_query: Option<(u64, u64)>,
    event_ai_rows: Vec<[u32; 24]>,
    script_library_query_line: Option<u64>,
    event_ai_summon_query: Option<(u64, u64)>,
}
impl Inputs {
    pub(super) fn bind_spawn_data(
        &mut self,
        catalog: &mut VerifiedCatalog,
        inputs: &mut EffectiveSpawnInputs,
    ) -> Vec<[u32; 4]> {
        let (templates, addons) = std::mem::take(&mut self.spawn_data).load(catalog, inputs);
        self.loaded_spawn_templates = Some(templates);
        addons
    }
    pub(super) fn observe_thread(&mut self, thread: u64, line: u64, kind: &str, p: &[&str]) {
        self.equipment.observe_map(thread, line, kind, p);
        if thread == 6 {
            let expected: Option<(&str, &[&str])> = match line {
                60637563 => Some((
                    "script_factory_argument",
                    &["145", "gameobject", "25769804506"],
                )),
                60637582 => Some((
                    "operation_begin",
                    &["7970", "7969", "TerrainInfo::GetAreaInfo"],
                )),
                60637844 => Some((
                    "geometry_area_result",
                    &["7970", "10757", "2", "138", "756"],
                )),
                60637854 => Some((
                    "operation_begin",
                    &["7977", "7969", "TerrainInfo::GetAreaInfo"],
                )),
                60638050 => Some((
                    "geometry_area_result",
                    &["7977", "10757", "2", "138", "756"],
                )),
                60638060 => Some(("active_add", &["0", "0x200c01f1a3f0", "15508"])),
                60638066 => Some((
                    "callback_result",
                    &["7969", "pointer", "0x200c01f1ad00", "25769804509"],
                )),
                60638080 => Some(("callback_dispatch", &["7984"])),
                60638101 => Some(("callback_dispatch", &["7985"])),
                60638112 => Some(("constructor_loader_success", &["25769804506", "15508"])),
                _ => None,
            };
            if let Some((trace_kind, prefix)) = expected {
                assert_eq!(kind, trace_kind);
                assert!(p.starts_with(prefix));
                match line {
                    60638080 => assert_eq!(p.last(), Some(&"JustSpawned")),
                    60638101 => assert_eq!(p.last(), Some(&"OnObjectSpawn")),
                    _ => {}
                }
                self.map_threads
                    .entry(thread)
                    .or_default()
                    .bell_trace
                    .push(line);
            }
        }
        if matches!(
            (thread, line, kind),
            (6, 60402993, "geometry_area_result") | (7, 60354456, "geometry_area_result")
        ) {
            assert!(self
                .map_threads
                .entry(thread)
                .or_default()
                .wmo_area_hit
                .replace(p.join(" "))
                .is_none());
        }
        if thread == 1 && kind == "sql_request_bytes" {
            let query = unhex(p.last().unwrap());
            if query == CORPSE_LOAD_QUERY.as_bytes() {
                assert!(self.corpse_query_line.replace(line).is_none());
            }
            if query == RANDOM_TEMPLATE_QUERY.as_bytes() {
                assert!(self.random_template_request_line.replace(line).is_none());
            }
        }
        if thread == 1 && kind == "mt_raw" {
            self.world_raw_words += 1;
        }
        if thread == 1 {
            self.templates.observe(line, kind, p, self.world_raw_words);
            self.spawn_data.observe(line, kind, p, self.world_raw_words);
            self.creature_models
                .observe(line, kind, p, self.world_raw_words);
            self.equipment.observe(line, kind, p, self.world_raw_words);
            self.spell_lists
                .observe(line, kind, p, self.world_raw_words);
            self.immunities.observe(line, kind, p, self.world_raw_words);
            self.script_registry
                .observe(line, kind, p, self.world_raw_words);
        }
        if thread == 1 && kind == "sql_query" {
            let query = String::from_utf8(unhex(p.last().unwrap())).unwrap();
            if query == EVENT_AI_QUERY {
                assert!(self
                    .event_ai_query
                    .replace((line, self.world_raw_words))
                    .is_none());
            }
            if query.contains(" FROM script_texts") {
                assert!(self.script_library_query_line.replace(line).is_none());
            }
            if query == "SELECT id, position_x, position_y, position_z, orientation, spawntimesecs FROM creature_ai_summons" {
                assert!(self.event_ai_summon_query.replace((line, self.world_raw_words)).is_none());
            }
            if GROUP_QUERIES.contains(&query.as_str()) {
                self.group_queries.push((line, query.clone()));
            }
            if WAYPOINT_MANAGER_QUERIES.contains(&query.as_str()) {
                self.waypoint_queries.push((line, query.clone()));
            }
            if ENCOUNTER_QUERIES.contains(&query.as_str()) {
                self.encounter_queries.push((line, query.clone()));
                assert!(self
                    .encounter_sql
                    .insert(query.clone(), Vec::new())
                    .is_none());
                self.encounter_raw
                    .insert(query, (self.world_raw_words, self.world_raw_words));
            }
        }
        if thread == 1 && kind == "sql_field" && p[0] == self.pointer {
            let query = std::str::from_utf8(&self.query).unwrap();
            if query == EVENT_AI_QUERY {
                let column: usize = p[1].parse().unwrap();
                if column == 0 {
                    self.event_ai_rows.push([0; 24]);
                }
                self.event_ai_rows.last_mut().unwrap()[column] =
                    std::str::from_utf8(&unhex(p.last().unwrap()))
                        .unwrap()
                        .parse::<i64>()
                        .unwrap() as u32;
                assert_eq!(self.event_ai_query.unwrap().1, self.world_raw_words);
            }
            if query == RANDOM_TEMPLATE_QUERY {
                let column: usize = p[1].parse().unwrap();
                if column == 0 {
                    self.random_template_rows.push([0; 4]);
                }
                self.random_template_rows.last_mut().unwrap()[column] =
                    std::str::from_utf8(&unhex(p.last().unwrap()))
                        .unwrap()
                        .parse::<i64>()
                        .unwrap() as u32;
            }
            if let Some(rows) = self.encounter_sql.get_mut(query) {
                let column: usize = p[1].parse().unwrap();
                if column == 0 {
                    rows.push(Vec::new());
                }
                let row = rows.last_mut().unwrap();
                assert_eq!(row.len(), column);
                row.push(unhex(p.last().unwrap()));
                self.encounter_raw.get_mut(query).unwrap().1 = self.world_raw_words;
            }
        }
        if kind == "sql_query" && unhex(p.last().unwrap()) == GRAVEYARD_QUERY.as_bytes() {
            assert_eq!(thread, 1);
            assert!(self.graveyard_query_line.replace(line).is_none());
            self.graveyard_raw_start = Some(self.world_raw_words);
        }
        if thread == 1
            && kind == "sql_field"
            && p[0] == self.pointer
            && self.query == GRAVEYARD_QUERY.as_bytes()
        {
            self.graveyard_last_field_line = Some(line);
            self.graveyard_raw_end = Some(self.world_raw_words);
        }
        if kind == "sql_query" && unhex(p.last().unwrap()) == b"SELECT event FROM game_event_status"
        {
            self.event_initialize_line.get_or_insert(line);
        }
        if kind == "sql_write_bytes" {
            let sql = String::from_utf8(unhex(p.last().unwrap())).unwrap();
            if sql.contains("game_event_status") {
                self.event_status_writes.push((line, sql));
            }
        }
        if kind == "logical_birth" && p[1] == "map" {
            self.map_identities
                .insert(p[0].parse().unwrap(), (line, p[4].into()));
        }
        if kind == "constructor_persistent_state" && p[1] == "used_map_binding" {
            self.map_bindings
                .insert(p[2].parse().unwrap(), (thread, p[4].parse().unwrap(), line));
        }
        if !matches!(thread, 6 | 7) {
            return;
        }
        let evidence = self.map_threads.entry(thread).or_default();
        match kind {
            "state_spawn_state" if p[1] == "initialize_end" => {
                evidence.collecting_spawn_groups = true;
            }
            "state_spawn_group" if evidence.collecting_spawn_groups && p[1] == "id" => {
                assert!(evidence.spawn_groups.insert(p[2].parse().unwrap()));
            }
            "operation_end" => evidence.collecting_spawn_groups = false,
            "geometry_detour_query_binding" if evidence.first_mmap_query.is_none() => {
                assert_eq!(p[1], "0");
                evidence.first_mmap_query = Some((line, p[4].parse().unwrap()));
            }
            "grid_create" if evidence.first_grid.is_none() => {
                evidence.first_grid = Some((line, p[2].parse().unwrap(), p[3].parse().unwrap()));
            }
            "logical_birth"
                if p[1] == "object"
                    && (thread == 6 && (60476953..MAP0_NEXT_OWNER_LINE).contains(&line)
                        || thread == 7 && (60310149..MAP1_NEXT_OWNER_LINE).contains(&line)) =>
            {
                evidence.continuation_births.push(line);
            }
            "constructor_loader_success"
                if thread == 6 && (60478609..MAP0_NEXT_OWNER_LINE).contains(&line)
                    || thread == 7 && (60435155..MAP1_NEXT_OWNER_LINE).contains(&line) =>
            {
                evidence
                    .continuation_completed
                    .push((line, p[1].parse().unwrap()));
            }
            "path_calculate"
                if thread == 6 && (60478609..MAP0_NEXT_OWNER_LINE).contains(&line)
                    || thread == 7 && (60310149..MAP1_NEXT_OWNER_LINE).contains(&line) =>
            {
                evidence.path_calculations.push(line);
            }
            "active_add" if evidence.active_adds.len() < if thread == 6 { 4 } else { 14 } => {
                evidence.active_adds.push((
                    line,
                    p[0].parse().unwrap(),
                    p[1].into(),
                    p[2].parse().unwrap(),
                ));
            }
            "grid_lock_inc" if evidence.active_locks.len() < if thread == 6 { 4 } else { 14 } => {
                evidence.active_locks.push((line, p[1].parse().unwrap()));
            }
            "constructor_loader_candidate" if evidence.first_candidate.is_none() => {
                evidence.first_candidate = Some((
                    line,
                    p[0].parse().unwrap(),
                    p[1].parse().unwrap(),
                    p[4].parse().unwrap(),
                    p[5].parse().unwrap(),
                ));
            }
            "object_identity"
                if evidence.first_candidate.is_some() && evidence.first_created_guid.is_none() =>
            {
                evidence.first_created_guid = Some((
                    line,
                    p[1].parse().unwrap(),
                    p[2].parse().unwrap(),
                    p[3].parse().unwrap(),
                    p[4].parse().unwrap(),
                ));
            }
            "rng_call"
                if evidence.first_candidate.is_some()
                    && matches!(p[0], "urand" | "irand" | "urand_default") =>
            {
                let bounds = if p[0] == "urand_default" {
                    [0, 0]
                } else {
                    [p[3].parse().unwrap(), p[4].parse().unwrap()]
                };
                evidence.candidate_requests.push((
                    evidence.total_words,
                    p[0].into(),
                    bounds[0],
                    bounds[1],
                    None,
                ));
                if evidence.first_candidate_rng.len() < if thread == 6 { 1 } else { 8 } {
                    evidence.first_candidate_rng.push((
                        p[0].into(),
                        p[2].parse().unwrap(),
                        bounds[0],
                        bounds[1],
                        None,
                    ));
                }
            }
            "rng_result"
                if evidence.first_candidate.is_some()
                    && matches!(p[0], "urand" | "irand" | "urand_default") =>
            {
                if let Some(last) = evidence.candidate_requests.last_mut() {
                    assert_eq!(last.1, p[0]);
                    last.4 = Some(p[1].parse().unwrap());
                }
                if let Some(last) = evidence.first_candidate_rng.last_mut() {
                    if last.4.is_none() {
                        assert_eq!(last.0, p[0]);
                        last.4 = Some(p[1].parse().unwrap());
                    }
                }
            }
            "clock_callsite" => {
                // Frozen ELF time@plt return PC 0x6f33d8; the instrumented line is :82.
                evidence.spawn_clock_site = p[0] == "1"
                    && p[1] == "3678789157770927477"
                    && p[2] == "7287768"
                    && p[3].ends_with("/bin/mangosd");
                if evidence.first_candidate.is_some() {
                    assert!(evidence
                        .active_clock_site
                        .replace((
                            line,
                            [
                                p[0].parse().unwrap(),
                                p[1].parse().unwrap(),
                                p[2].parse().unwrap(),
                            ],
                            p[3].ends_with("/bin/mangosd"),
                        ))
                        .is_none());
                }
            }
            "clock_time" => {
                if evidence.spawn_clock_site {
                    assert!(evidence
                        .spawn_clock
                        .replace((line, p[0].parse().unwrap(), evidence.total_words))
                        .is_none());
                    evidence.spawn_clock_site = false;
                }
                if evidence.first_candidate.is_some() {
                    let (site_line, site, module) = evidence
                        .active_clock_site
                        .take()
                        .expect("active clock_time requires its preceding clock_callsite");
                    evidence
                        .active_clocks
                        .0
                        .push_back(active_init::ActiveClock {
                            site_line,
                            time_line: line,
                            thread,
                            site,
                            module,
                            seconds: p[0].parse().unwrap(),
                            raw: evidence.total_words as u64,
                        });
                }
            }
            "mt_state" => {
                assert!(evidence.initial.is_none());
                evidence.initial = Some(benilla_sim::source_rng::MtState {
                    words: std::array::from_fn(|i| p[i + 1].parse().unwrap()),
                    index: p[625].parse().unwrap(),
                });
            }
            "mt_raw" => {
                let word = (line, p[1].parse().unwrap(), p[2].parse().unwrap());
                evidence.words.push(word);
                evidence.first_word.get_or_insert(word);
                if evidence.sql_line.is_none() {
                    evidence.before_sql.push(word);
                }
                evidence.total_words += 1;
            }
            "sql_request_bytes" | "sql_write_bytes" => {
                if thread == 6 && line < MAP0_NEXT_OWNER_LINE
                    || thread == 7 && line < MAP1_NEXT_OWNER_LINE
                {
                    evidence.sql_lines.push(line);
                }
                if evidence.sql_statements.len() < 2 {
                    assert_eq!(p[4], "0");
                    let bytes = p[5]
                        .as_bytes()
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|pair| {
                            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(bytes.len(), p[3].parse::<usize>().unwrap());
                    evidence.sql_statements.push(bytes);
                }
                if kind == "sql_request_bytes" {
                    evidence.sql_line.get_or_insert(line);
                }
            }
            "rng_shuffle_begin" => {
                evidence.total_shuffles += 1;
                evidence.first_shuffle.get_or_insert_with(|| p[0].into());
            }
            "rng_shuffle_member" if evidence.first_shuffle.as_deref() == Some(p[0]) => {
                let values = if p[1] == "before" {
                    &mut evidence.shuffle_before
                } else {
                    &mut evidence.shuffle_after
                };
                assert_eq!(values.len(), p[2].parse::<usize>().unwrap());
                values.push(p[3].parse().unwrap());
            }
            "constructor_persistent_pool_state" if p[1] == "1" => {
                assert!(evidence.pool_snapshot_line.replace(line).is_none());
                assert!(evidence
                    .pool_raw_end
                    .replace(evidence.total_words)
                    .is_none());
                assert!(evidence
                    .pool_shuffles
                    .replace(evidence.total_shuffles)
                    .is_none());
            }
            "constructor_persistent_pool_member" => {
                let index = match p[1] {
                    "creature" => 0,
                    "gameobject" => 1,
                    _ => panic!("unknown pool member"),
                };
                assert!(evidence.pool_members[index].insert(p[2].parse().unwrap()));
            }
            "constructor_persistent_pool_count" => {
                assert!(evidence
                    .pool_counts
                    .insert(p[1].parse().unwrap(), p[2].parse().unwrap())
                    .is_none());
            }
            _ => (),
        }
    }
    pub(super) fn observe(&mut self, kind: &str, p: &[&str]) {
        match kind {
            "logical_birth" if p[1] == "map" => self.map_births += 1,
            "constructor_persistent_add" => self
                .persistent_creations
                .push((p[0].parse().unwrap(), p[1].parse().unwrap())),
            "sql_query_request" => self.query_operation = p[1].parse().unwrap(),
            "sql_query" => {
                self.query = unhex(p.last().unwrap());
                self.pointer = p[0].to_owned();
                self.event = 0;
                self.query_occurrence += 1;
                for (slot, query, line, function) in [
                    (0, &b"SELECT guid, respawntime, map, instance, resettime, encountersMask FROM creature_respawn LEFT JOIN instance ON instance = id"[..], 997, "MapPersistentStateManager::LoadCreatureRespawnTimes"),
                    (1, &b"SELECT guid, respawntime, map, instance, resettime, encountersMask FROM gameobject_respawn LEFT JOIN instance ON instance = id"[..], 1062, "MapPersistentStateManager::LoadGameobjectRespawnTimes"),
                ] {
                    if self.query == query {
                        assert!(self.persistent_owners[slot].is_none());
                        self.persistent_owners[slot] = Some(InputOwner {
                            thread: 1, occurrence: self.query_occurrence,
                            operation: OperationId { thread: 1, ordinal: self.query_operation },
                            map_instance: None, object: None,
                            callsite: Callsite {
                                core_revision: "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f".into(),
                                source_file: "src/game/Maps/MapPersistentStateMgr.cpp".into(), line,
                                function: function.into(), inlined_caller_chain: Vec::new(),
                                site_ordinal: self.query_occurrence,
                            },
                        });
                    }
                }
            }
            "sql_field" if p[0] == self.pointer && self.query == GRAVEYARD_QUERY.as_bytes() => {
                let column = p[1].parse::<usize>().unwrap();
                if column == 0 {
                    self.graveyard_rows.push([0; 4]);
                }
                self.graveyard_rows.last_mut().unwrap()[column] =
                    std::str::from_utf8(&unhex(p.last().unwrap()))
                        .unwrap()
                        .parse()
                        .unwrap();
            }
            "sql_field"
                if p[0] == self.pointer
                    && p[1] == "0"
                    && (self.query.starts_with(b"SELECT entry,schedule_type,")
                        || self.query
                            == b"SELECT entry, start_time, end_time FROM game_event_time") =>
            {
                self.event = std::str::from_utf8(&unhex(p.last().unwrap()))
                    .unwrap()
                    .parse()
                    .unwrap();
            }
            "clock_callsite" if p[1] == "3678789157770927477" => {
                self.clock_site = p[2].parse().unwrap()
            }
            "clock_callsite" => self.clock_site = 0,
            "clock_time" => {
                let value = p[0].parse().unwrap();
                // Addresses resolve in the captured module-1 ELF, including inlined callers.
                match self.clock_site {
                    0x8329fa => self.world_seconds = Some(value), // World constructor, before respawn loading.
                    0xa40e8e => self.load_clocks.push_back((
                        EventClockSite::LoadSchedule(self.event),
                        ClockSample::authored(ClockKind::WallSeconds, value),
                    )),
                    0xa41f27 => self.load_clocks.push_back((
                        EventClockSite::LoadYearly(self.event),
                        ClockSample::authored(ClockKind::WallSeconds, value),
                    )),
                    0xa45ec1 | 0xa45f3e if self.lunar() => {
                        self.calendar.push_back(CalendarRecord::Now(value))
                    }
                    0xa3fd72 => {
                        self.update_clocks.push_back(value);
                        self.updating = true;
                    }
                    0xa400bb => self.check_clocks.push_back(value),
                    _ => (),
                }
            }
            "clock_localtime" if self.lunar() => self
                .calendar
                .push_back(CalendarRecord::Local(p[0].parse().unwrap(), tm(&p[1..]))),
            "clock_mktime_input" if self.lunar() => {
                assert_eq!(p.len(), 7);
                self.epoch = Some(std::array::from_fn(|i| p[i].parse().unwrap()));
            }
            "clock_mktime_result" if self.lunar() => {
                self.calendar.push_back(CalendarRecord::Epoch(
                    self.epoch.take().unwrap(),
                    p[0].parse().unwrap(),
                    tm(&p[1..]),
                ))
            }
            "sql_write_bytes" if self.updating => {
                let sql = String::from_utf8(unhex(p.last().unwrap())).unwrap();
                if sql.contains("game_event_status") {
                    self.expected_sql.push(sql);
                }
            }
            _ => (),
        }
    }
    fn lunar(&self) -> bool {
        self.event == 7 && self.query.starts_with(b"SELECT entry,schedule_type,")
    }
}
struct Calendar(VecDeque<CalendarRecord>);
impl moon::Calendar for Calendar {
    fn owner(&self) -> moon::CallOwner {
        moon::CallOwner { thread: 1, call: 7 }
    }
    fn now(&mut self) -> moon::Result<i64> {
        match self.0.pop_front() {
            Some(CalendarRecord::Now(value)) => Ok(value),
            _ => Err(moon::Error::MissingObservation),
        }
    }
    fn local(&mut self, seconds: i64) -> moon::Result<Tm> {
        match self.0.pop_front() {
            Some(CalendarRecord::Local(expected, value)) if expected == seconds => Ok(value),
            _ => Err(moon::Error::ObservationMismatch),
        }
    }
    fn epoch(&mut self, input: moon::CalendarInput) -> moon::Result<(i64, Tm)> {
        let fields = match input {
            moon::CalendarInput::Retained(t) => {
                [t.sec, t.min, t.hour, t.mday, t.mon, t.year, t.isdst]
            }
            moon::CalendarInput::AnchorProbe(d) => [0, 0, 3, d.day, d.month - 1, d.year - 1900, -1],
        };
        match self.0.pop_front() {
            Some(CalendarRecord::Epoch(expected, seconds, normalized)) if expected == fields => {
                Ok((seconds, normalized))
            }
            other => {
                println!(
                    "F6_FIRST_DIVERGENCE calendar_input expected={other:?} got={fields:?} candidate_causes=lunar_arithmetic,sine_dispatch,calendar_order"
                );
                Err(moon::Error::ObservationMismatch)
            }
        }
    }
    fn finish(&self) -> moon::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(moon::Error::SurplusObservation)
        }
    }
}
struct Definitions<'a> {
    spawns: &'a SourceEffectiveSpawns,
    maps: &'a BTreeMap<u32, u32>,
}
impl PersistentDefinitions for Definitions<'_> {
    fn map_entries(&mut self) -> Result<Vec<PersistentMapEntry>> {
        Ok(self
            .maps
            .iter()
            .map(|(&id, &map_type)| PersistentMapEntry { id, map_type })
            .collect())
    }
    fn spawn_map(&mut self, kind: GridSpawnKind, guid: u32) -> Result<Option<u32>> {
        Ok(self
            .spawns
            .pre_terrain_data(SpawnKey { kind, guid })?
            .map(|d| d.row.map))
    }
}
struct InitialEffects<'a> {
    initial: &'a ManagerInitialInputs<'a>,
    operations: Vec<PersistentOperation>,
    outbound: Vec<PersistentSql>,
    cleaned: [bool; 2],
}
impl PersistentEffects for InitialEffects<'_> {
    fn sql(&mut self, sql: PersistentSql) -> Result<()> {
        if let PersistentSql::Cleanup {
            kind,
            database_seconds,
        } = sql
        {
            require(
                database_seconds == self.initial.query(kind).database_seconds,
                "execution.persistent.cleanup_clock",
            )?;
            self.cleaned[usize::from(kind == GridSpawnKind::GameObject)] = true;
            Ok(())
        } else if matches!(sql, PersistentSql::CreateWorld { .. }) {
            self.outbound.push(sql);
            Ok(())
        } else {
            Err(refuse("execution.persistent.initial_write_owner"))
        }
    }
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus> {
        require(
            self.cleaned[usize::from(kind == GridSpawnKind::GameObject)],
            "execution.persistent.cleanup_owner",
        )?;
        Ok(self.initial.frozen_query_result(kind))
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.operations.push(operation);
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("execution.persistent.dungeon_reset_owner"))
    }
}
struct Owner<'a> {
    spawns: &'a SourceEffectiveSpawns,
    maps: &'a BTreeMap<u32, u32>,
    ids: [BTreeSet<u32>; 5],
    lunar: ReferenceLunarEventOwner<'a>,
    clocks: VecDeque<(EventClockSite, ClockSample)>,
    update: VecDeque<i64>,
    checks: VecDeque<i64>,
    clock_count: usize,
    pools: SourcePoolManager,
    states: SourceManagers,
    loaded_maps: BTreeMap<(u32, u32), MapIdentity>,
    effects: SourceEventEffects,
    quests: BTreeMap<u32, bool>,
    quest_activity: SourceQuestActivity,
    missing_quest_activity_writes: usize,
    status: BTreeSet<u16>,
    sql: Vec<String>,
    pool_operations: Vec<PoolOperation>,
    observations: Vec<GameEventObservation>,
    link_errors: Vec<PoolEventLinkMismatch>,
    engine: &'a mut Mt19937,
    cursor: &'a mut DrawCursor,
    raw: Vec<(u64, u64)>,
}
impl GameEventOwner for Owner<'_> {
    fn clock(&mut self, site: EventClockSite) -> Result<ClockSample> {
        self.clock_count += 1;
        match site {
            EventClockSite::LoadSchedule(_) | EventClockSite::LoadYearly(_) => {
                let (expected, sample) = self
                    .clocks
                    .pop_front()
                    .ok_or_else(|| refuse("execution.event.recorded_load_clock_missing"))?;
                require(
                    expected == site,
                    "execution.event.recorded_load_clock_order",
                )?;
                Ok(sample)
            }
            EventClockSite::Update => self
                .update
                .pop_front()
                .map(|n| ClockSample::authored(ClockKind::WallSeconds, n))
                .ok_or_else(|| refuse("execution.event.recorded_update_clock_missing")),
            EventClockSite::NextCheck(_) => self
                .checks
                .pop_front()
                .map(|n| ClockSample::authored(ClockKind::WallSeconds, n))
                .ok_or_else(|| refuse("execution.event.recorded_check_clock_missing")),
            _ => Err(refuse("execution.event.recorded_clock_owner_missing")),
        }
    }
    fn lunar_bindings(&mut self) -> Result<LunarEventBindings<'_>> {
        Ok(self.lunar.bindings())
    }
    fn reference(&mut self, kind: EventReference, id: u32) -> Result<bool> {
        Ok(self.ids[kind as usize].contains(&id))
    }
    fn set_quest_active(&mut self, quest: u32, active: bool) -> Result<()> {
        let result = self.quest_activity.set_active(quest, active);
        self.missing_quest_activity_writes += usize::from(result.is_err());
        result?;
        self.quests.insert(quest, active);
        Ok(())
    }
    fn check_pool_event(
        &mut self,
        pools: &SourcePoolManager,
        pool: u16,
        event: i16,
        creatures: &BTreeMap<u32, i16>,
        gameobjects: &BTreeMap<u32, i16>,
    ) -> Result<()> {
        self.link_errors
            .extend(pools.check_event_links(pool, event, creatures, gameobjects)?);
        Ok(())
    }
    fn effect(
        &mut self,
        manager: &mut SourceGameEventManager,
        effect: GameEventEffect,
    ) -> Result<()> {
        let mut effects = std::mem::take(&mut self.effects);
        let mut pools = std::mem::take(&mut self.pools);
        let mut states = std::mem::take(&mut self.states);
        let result = effects.execute(manager, effect, 1, &mut pools, &mut states, self);
        self.effects = effects;
        self.pools = pools;
        self.states = states;
        result
    }
    fn observe(&mut self, observation: GameEventObservation) -> Result<()> {
        self.observations.push(observation);
        Ok(())
    }
}
impl PoolWork for Owner<'_> {
    fn engine_access(&mut self) -> Result<()> {
        Ok(())
    }
    fn chance(&mut self) -> Result<f64> {
        let mut sink = |e: TraceEvent| {
            if let EventKind::Raw { position, word, .. } = e.kind {
                self.raw.push((position, word));
            }
        };
        SourceRandom::new(1, self.engine, self.cursor, &mut sink)
            .unwrap()
            .rand_chance()
            .map_err(|_| refuse("execution.pool.rand_chance"))
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<()> {
        let mut sink = |e: TraceEvent| {
            if let EventKind::Raw { position, word, .. } = e.kind {
                self.raw.push((position, word));
            }
        };
        SourceRandom::new(1, self.engine, self.cursor, &mut sink)
            .unwrap()
            .shuffle(values)
            .map_err(|_| refuse("execution.pool.shuffle"))
    }
    fn can_spawn(
        &mut self,
        _: &mut SourcePoolManager,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        _: PoolKind,
        _: u32,
    ) -> Result<bool> {
        require(
            states.state(state)?.map_binding().is_none(),
            "execution.pool.map_can_spawn_owner",
        )?;
        Ok(true)
    }
    fn leaf_effect(
        &mut self,
        _: &mut SourcePoolManager,
        states: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()> {
        execute_pool_leaf(states, effect, self)
    }
    fn observe(&mut self, operation: PoolOperation) -> Result<()> {
        self.pool_operations.push(operation);
        Ok(())
    }
}
impl PoolStateOwner for Owner<'_> {
    fn pool_map_entry(&mut self, map: u32) -> Result<Option<PersistentMapEntry>> {
        Ok(self
            .maps
            .get(&map)
            .map(|&map_type| PersistentMapEntry { id: map, map_type }))
    }
    fn pool_next_instance_state(
        &mut self,
        states: &SourceManagers,
        map: u32,
        _: Option<PersistentStateIdentity>,
    ) -> Result<Option<PersistentStateIdentity>> {
        require(
            !states
                .inventories()
                .iter()
                .any(|s| s.identity.map == map && s.identity.instance != 0),
            "execution.pool.instance_state_traversal_owner",
        )?;
        Ok(None)
    }
}
impl EventEffectOwner for Owner<'_> {
    fn event_definition(&mut self, key: SpawnKey) -> Result<Option<EventSpawnDefinition>> {
        Ok(self
            .spawns
            .pre_terrain_group_definition(key)?
            .map(|d| EventSpawnDefinition {
                map: d.map,
                cell: d.cell,
                entry: d.entry,
                guid: u64::from(key.guid),
            }))
    }
    fn event_next_map(
        &mut self,
        map_id: Option<u32>,
        after: Option<MapIdentity>,
    ) -> Result<Option<MapIdentity>> {
        Ok(self
            .loaded_maps
            .values()
            .find(|map| {
                map_id.is_none_or(|id| id == map.map)
                    && after.is_none_or(|previous| {
                        (map.map, map.instance) > (previous.map, previous.instance)
                    })
            })
            .copied())
    }
    fn event_post_map(&mut self, _: MapIdentity, _: MessageEnvelope) -> Result<()> {
        Err(refuse("execution.event.map_traversal_owner"))
    }
    fn event_sql(&mut self, _: u64, sql: &str) -> Result<()> {
        if sql == "TRUNCATE TABLE game_event_status" {
            self.status.clear();
        } else if let Some(id) = sql
            .strip_prefix("INSERT INTO game_event_status (event) VALUES (")
            .and_then(|s| s.strip_suffix(')'))
        {
            self.status.insert(
                id.parse()
                    .map_err(|_| refuse("execution.event.status_sql"))?,
            );
        } else if let Some(id) = sql.strip_prefix("DELETE FROM game_event_status WHERE event = ") {
            self.status.remove(
                &id.parse()
                    .map_err(|_| refuse("execution.event.status_sql"))?,
            );
        } else {
            return Err(refuse("execution.event.status_sql"));
        }
        self.sql.push(sql.to_owned());
        Ok(())
    }
    fn event_announce(&mut self, _: u16, _: &[u8]) -> Result<()> {
        Err(refuse("execution.event.startup_announce_owner"))
    }
    fn event_quest(&mut self, quest: u32, active: bool) -> Result<()> {
        self.set_quest_active(quest, active)
    }
    fn event_has_instance(&mut self, _: MapIdentity) -> Result<bool> {
        Err(EventPending::Instance.refusal())
    }
}
impl PersistentEffects for Owner<'_> {
    fn sql(&mut self, _: PersistentSql) -> Result<()> {
        Err(refuse("execution.pool.respawn_storage_owner"))
    }
    fn query_respawns(&mut self, _: GridSpawnKind) -> Result<CharacterQueryStatus> {
        Err(refuse("execution.pool.respawn_query_unreached"))
    }
    fn observe(&mut self, _: PersistentOperation) -> Result<()> {
        Ok(())
    }
    fn dungeon_reset(&mut self, _: PersistentMapEntry) -> Result<i64> {
        Err(refuse("execution.persistent.dungeon_reset_owner"))
    }
}
impl PoolLeafOwner for Owner<'_> {
    fn pool_spawn(
        &mut self,
        kind: GridSpawnKind,
        guid: u32,
    ) -> Result<Option<PoolSpawnDefinition>> {
        let key = SpawnKey { kind, guid };
        let Some(data) = self.spawns.pre_terrain_data(key)? else {
            return Ok(None);
        };
        let cell = self.spawns.pre_terrain_group_definition(key)?.unwrap().cell;
        Ok(Some(PoolSpawnDefinition {
            map: data.row.map,
            cell,
            entry: data.row.entry,
            respawn: data.row.respawn,
            gameobject_min: data.row.respawn[0] as i32,
        }))
    }
    fn pool_grid_loaded(
        &mut self,
        _: MapIdentity,
        _: benilla_sim::resident_map::CellPair,
    ) -> Result<bool> {
        Err(refuse("execution.pool.loaded_grid_owner"))
    }
    fn pool_random_respawn(&mut self, bounds: [u32; 2]) -> Result<u32> {
        let mut sink = |e: TraceEvent| {
            if let EventKind::Raw { position, word, .. } = e.kind {
                self.raw.push((position, word));
            }
        };
        SourceRandom::new(1, self.engine, self.cursor, &mut sink)
            .unwrap()
            .urand(bounds[0], bounds[1])
            .map_err(|_| refuse("execution.pool.respawn_rng"))
    }
    fn pool_wall(&mut self, _: PoolLeafClock) -> Result<ClockSample> {
        Err(refuse("execution.pool.wall_clock_owner"))
    }
    fn pool_world_seconds(&mut self) -> Result<i64> {
        Err(refuse("execution.pool.world_seconds_owner"))
    }
    fn pool_load(
        &mut self,
        _: PoolLoadRequest,
        _: &mut SourceManagers,
    ) -> Result<GridCandidateOutcome> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_lookup(&mut self, _: MapIdentity, _: SpawnKey) -> Result<Option<ObjectHandle>> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_live(&mut self, _: PoolLiveRequest, _: &mut SourceManagers) -> Result<()> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_save_immediately(&mut self) -> Result<bool> {
        Err(refuse("execution.pool.respawn_config_owner"))
    }
    fn pool_creature_world_boss(&mut self, _: ObjectHandle) -> Result<bool> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_gameobject_dynamic(&mut self, _: u32) -> Result<Option<bool>> {
        Err(PoolPending::LeafEffects.refusal())
    }
    fn pool_schedule(&mut self, _: PoolScheduleRequest, _: &mut SourceManagers) -> Result<()> {
        Err(PoolPending::LeafEffects.refusal())
    }
}

struct LinkingDefinitions<'a> {
    catalog: &'a mut VerifiedCatalog,
    spawns: &'a SourceEffectiveSpawns,
    inputs: &'a EffectiveSpawnInputs,
}
fn linking_spawn(spawns: &SourceEffectiveSpawns, guid: u32) -> Result<Option<LinkingSpawnData>> {
    Ok(spawns
        .pre_terrain_data(SpawnKey {
            kind: GridSpawnKind::Creature,
            guid,
        })?
        .map(|data| LinkingSpawnData {
            entry: data.row.entry,
            map: data.row.map,
            respawn_xy: [data.row.pose[0].to_bits(), data.row.pose[1].to_bits()],
        }))
}
impl LinkingLoadOwner for LinkingDefinitions<'_> {
    fn template_exists(&mut self, entry: u32) -> Result<bool> {
        Ok(self.inputs.creatures.contains_key(&entry))
    }
    fn map_exists(&mut self, map: u32) -> Result<bool> {
        Ok(self.inputs.maps.contains_key(&map))
    }
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>> {
        linking_spawn(self.spawns, guid)
    }
    fn query_unique_master(&mut self, entry: u32, map: u32) -> Result<LinkingMasterQuery> {
        query_catalog_linking_master(self.catalog, entry, map)
    }
}
struct MapLinkingGate<'a> {
    spawns: &'a SourceEffectiveSpawns,
    pools: &'a SourcePoolManager,
    script: Option<&'a SourceWorldScript>,
}
impl LinkingGateOwner for MapLinkingGate<'_> {
    fn creature_data(&mut self, guid: u32) -> Result<Option<LinkingSpawnData>> {
        linking_spawn(self.spawns, guid)
    }
    fn wall_time(&mut self, _: MapIdentity, _: u32) -> Result<ClockSample> {
        Err(refuse("execution.linking.wall_clock_owner"))
    }
    fn creature_pool(&mut self, guid: u32) -> Result<Option<u16>> {
        Ok(Some(self.pools.part_of(PoolKind::Creature, guid)?))
    }
    fn encounter_in_progress(&mut self, map: MapIdentity) -> Result<Option<bool>> {
        self.script
            .map(|script| {
                require(script.map == map, "execution.linking.instance_map_binding")?;
                Ok(script.encounter_in_progress())
            })
            .transpose()
    }
}
/// Independent map-job projection. Only RNG calls change stream; dependent owners stay real.
struct MapPoolWork<'a, 'b, 'r, S: FnMut(TraceEvent)> {
    world: &'a mut Owner<'b>,
    random: &'a mut SourceRandom<'r, Mt19937, S>,
    map: &'a SourceContinent,
    links: &'a SourceCreatureLinking,
    objects: &'a SourceObjects,
    shuffles: Vec<Vec<usize>>,
    persistent: Vec<PersistentOperation>,
}
impl<S: FnMut(TraceEvent)> PoolWork for MapPoolWork<'_, '_, '_, S> {
    fn engine_access(&mut self) -> Result<()> {
        self.world.engine_access()
    }
    fn chance(&mut self) -> Result<f64> {
        self.random
            .rand_chance()
            .map_err(|_| refuse("execution.pool.rand_chance"))
    }
    fn shuffle(&mut self, values: &mut [usize]) -> Result<()> {
        self.random
            .shuffle(values)
            .map_err(|_| refuse("execution.pool.shuffle"))?;
        self.shuffles.push(values.to_vec());
        Ok(())
    }
    fn can_spawn(
        &mut self,
        manager: &mut SourcePoolManager,
        states: &mut SourceManagers,
        state: PersistentStateIdentity,
        kind: PoolKind,
        guid: u32,
    ) -> Result<bool> {
        let kind = match kind {
            PoolKind::Creature => SourceObjectKind::Creature,
            PoolKind::GameObject => SourceObjectKind::GameObject,
            PoolKind::Child => return Ok(true),
        };
        self.map.can_spawn(
            kind,
            guid,
            self.links,
            self.objects,
            states.state(state)?,
            &mut MapLinkingGate {
                spawns: self.world.spawns,
                pools: manager,
                script: self.map.instance_data.as_ref(),
            },
        )
    }
    fn leaf_effect(
        &mut self,
        manager: &mut SourcePoolManager,
        states: &mut SourceManagers,
        effect: PoolLeafEffect,
    ) -> Result<()> {
        let _ = manager;
        execute_pool_leaf(states, effect, self)
    }
    fn observe(&mut self, operation: PoolOperation) -> Result<()> {
        PoolWork::observe(self.world, operation)
    }
}
impl<S: FnMut(TraceEvent)> PoolLeafOwner for MapPoolWork<'_, '_, '_, S> {
    fn pool_spawn(
        &mut self,
        kind: GridSpawnKind,
        guid: u32,
    ) -> Result<Option<PoolSpawnDefinition>> {
        self.world.pool_spawn(kind, guid)
    }
    fn pool_grid_loaded(
        &mut self,
        map: MapIdentity,
        cell: benilla_sim::resident_map::CellPair,
    ) -> Result<bool> {
        require(map == self.map.identity, "execution.pool.map_binding")?;
        self.map.grid_loaded(cell.grid_x(), cell.grid_y())
    }
    fn pool_random_respawn(&mut self, bounds: [u32; 2]) -> Result<u32> {
        self.random
            .urand(bounds[0], bounds[1])
            .map_err(|_| refuse("execution.pool.respawn_rng"))
    }
    fn pool_wall(&mut self, site: PoolLeafClock) -> Result<ClockSample> {
        self.world.pool_wall(site)
    }
    fn pool_world_seconds(&mut self) -> Result<i64> {
        self.world.pool_world_seconds()
    }
    fn pool_load(
        &mut self,
        request: PoolLoadRequest,
        states: &mut SourceManagers,
    ) -> Result<GridCandidateOutcome> {
        self.world.pool_load(request, states)
    }
    fn pool_lookup(&mut self, map: MapIdentity, key: SpawnKey) -> Result<Option<ObjectHandle>> {
        self.world.pool_lookup(map, key)
    }
    fn pool_live(&mut self, request: PoolLiveRequest, states: &mut SourceManagers) -> Result<()> {
        self.world.pool_live(request, states)
    }
    fn pool_save_immediately(&mut self) -> Result<bool> {
        self.world.pool_save_immediately()
    }
    fn pool_creature_world_boss(&mut self, object: ObjectHandle) -> Result<bool> {
        self.world.pool_creature_world_boss(object)
    }
    fn pool_gameobject_dynamic(&mut self, entry: u32) -> Result<Option<bool>> {
        self.world.pool_gameobject_dynamic(entry)
    }
    fn pool_schedule(
        &mut self,
        request: PoolScheduleRequest,
        states: &mut SourceManagers,
    ) -> Result<()> {
        self.world.pool_schedule(request, states)
    }
}
impl<S: FnMut(TraceEvent)> PersistentEffects for MapPoolWork<'_, '_, '_, S> {
    fn sql(&mut self, operation: PersistentSql) -> Result<()> {
        PersistentEffects::sql(self.world, operation)
    }
    fn query_respawns(&mut self, kind: GridSpawnKind) -> Result<CharacterQueryStatus> {
        self.world.query_respawns(kind)
    }
    fn observe(&mut self, operation: PersistentOperation) -> Result<()> {
        self.persistent.push(operation);
        Ok(())
    }
    fn dungeon_reset(&mut self, entry: PersistentMapEntry) -> Result<i64> {
        self.world.dungeon_reset(entry)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn continue_startup(
    recording: &Path,
    catalog: &mut VerifiedCatalog,
    spawns: &mut SourceEffectiveSpawns,
    inputs: &EffectiveSpawnInputs,
    gameobject_addons: &[[u32; 4]],
    pools: SourcePoolManager,
    event_occurrences: &Path,
    mut observed: Inputs,
    geometry: &mut SourceTerrainManager,
    engine: &mut Mt19937,
    cursor: &mut DrawCursor,
    next_word: (u64, u64),
) {
    let config = std::fs::read(recording.join("mangosd.conf")).unwrap();
    let (gameobject_templates, creature_templates, creature_models, equipment, mut dynamic_models) =
        std::mem::take(&mut observed.templates).load(
            catalog,
            recording,
            std::mem::take(&mut observed.creature_models),
            std::mem::take(&mut observed.equipment),
            geometry,
        );
    let creature_immunities =
        std::mem::take(&mut observed.immunities).load(catalog, &creature_templates);
    let creature_spell_lists =
        std::mem::take(&mut observed.spell_lists).load(catalog, &creature_templates);
    spell_list_reach::census(catalog, &creature_templates, spawns, &creature_spell_lists);
    let mut disabled_census = [0usize; 4];
    for template in creature_templates.rows() {
        let key = if template.words[89] != 0 {
            template.words[89]
        } else {
            template.words[0].wrapping_mul(100)
        };
        let slot = if key == 0 {
            0
        } else if let Some(list) = creature_spell_lists.get(key) {
            if list.disabled {
                1
            } else {
                2
            }
        } else {
            3
        };
        disabled_census[slot] += 1;
    }
    assert_eq!(
        disabled_census.iter().sum::<usize>(),
        creature_templates.rows().len()
    );
    assert_eq!(disabled_census, [0, 2290, 238, 7856]);
    println!(
        "F6_SPELL_LIST_DISABLED zero_written={} copied_disabled={} copied_enabled={} unwritten={} map1_4692_defined={} cold_start_disabled_decision_continue=10384 cold_start_disabled_decision_refuse=0 unwritten_nonempty_or_combat_refuse=7856",
        disabled_census[0], disabled_census[1], disabled_census[2], disabled_census[3],
        creature_spell_lists.get(469200).is_some()
    );
    let spawn_templates = observed.loaded_spawn_templates.take().unwrap();
    spawn_data::census(spawns, &spawn_templates, gameobject_addons);
    creature_models::entry_census(spawns, &creature_templates);
    let level_definitions = CreatureLevelDefinitions::load(catalog).unwrap();
    let level_config = CreatureLevelConfig::parse(&config, Sha256Id::of(&config)).unwrap();
    let faction_flags = creature_models::faction_flags(catalog);
    // World.cpp:1381, before MapManager initialization at :1389. Shared F5.01 identity.
    let script_report = std::env::var_os("BENILLA_F5_SCRIPT_REPORT").unwrap();
    let script_report = Path::new(&script_report);
    let script_report_hash = Sha256Id::of(&std::fs::read(script_report).unwrap());
    let mut ai_registry = AiRegistryBuilder::builtin();
    ai_registry
        .load_catalog_scripts(catalog, script_report)
        .unwrap();
    let ai_registry = ai_registry.freeze();
    let ai_factions = CreatureAiFactions::load_catalog(catalog).unwrap();
    let ai_config = EventAiLoaderConfig::parse(&config, Sha256Id::of(&config)).unwrap();
    let ai_templates = catalog_creature_ai_census_inputs(catalog)
        .unwrap()
        .into_iter()
        .map(|template| (template.entry, template))
        .collect::<BTreeMap<_, _>>();
    // World.cpp:1214 calls ScriptMgr.cpp:1228 before either EventAI load.
    let script_occurrences = std::env::var_os("BENILLA_F5_DB_SCRIPT_OCCURRENCES").unwrap();
    let script_occurrences = Path::new(&script_occurrences);
    let script_templates = catalog_db_script_templates(catalog, script_occurrences).unwrap();
    let template_query_line = observed.random_template_request_line.unwrap();
    assert_eq!(template_query_line, 52620961);
    // Trace :52621090 reports 418 rows. Compare occurrence order within each key.
    assert_eq!(observed.random_template_rows.len(), 418);
    assert!(script_templates.dropped().is_empty());
    let mut recorded_templates = BTreeMap::<_, Vec<_>>::new();
    for [id, kind, target, chance] in observed.random_template_rows {
        recorded_templates
            .entry((kind, id))
            .or_default()
            .push((target as i32, chance));
    }
    for ((kind, id), rows) in &recorded_templates {
        let loaded = match kind {
            0 => script_templates.string(*id),
            1 => script_templates.relay(*id),
            _ => panic!("unexpected stock random template type {kind}"),
        };
        assert_eq!(loaded, Some(rows.as_slice()), "type {kind}, template {id}");
    }
    assert_eq!(
        script_templates.string(12),
        Some([(73, 0), (74, 0), (75, 0), (76, 0)].as_slice())
    );
    // World.cpp:1316 loads the shared EventAI store before continent construction.
    let mut event_rows = catalog_event_ai_rows(catalog).unwrap();
    event_rows.sort_by_key(|row| row.words[0]);
    assert_eq!(
        event_rows.iter().map(|row| row.words).collect::<Vec<_>>(),
        observed.event_ai_rows
    );
    let event_query = observed.event_ai_query.unwrap();
    assert!(observed
        .map_identities
        .values()
        .all(|(line, _)| event_query.0 < *line));
    let mut event_definitions = EventAiDefinitions::default();
    let mut event_unavailable = BTreeSet::new();
    let mut event_skipped_sites = BTreeMap::<u32, usize>::new();
    let mut event_unavailable_causes = BTreeMap::<String, usize>::new();
    let mut event_loader = EventAiCatalog::new(catalog, ai_config.clone());
    event_loader.bind_spell_lists(&creature_spell_lists);
    event_loader.bind_script_templates(&script_templates);
    let mut event_census = [0usize; 3];
    for row in event_rows {
        let key = row.words[1] as i32;
        let id = row.words[0];
        let flags = row.words[5];
        match load_event_ai_row(&mut event_loader, row) {
            Ok(EventAiLoad {
                disposition: EventAiDisposition::Loaded(event),
                notices,
            }) => {
                assert!(notices.iter().all(|notice| notice.line != 992));
                event_census[0] += 1;
                event_definitions.insert(event);
            }
            Ok(EventAiLoad {
                disposition: EventAiDisposition::Skipped { line },
                ..
            }) => {
                event_census[1] += 1;
                *event_skipped_sites.entry(line).or_default() += 1;
            }
            Err(error) => {
                event_census[2] += 1;
                event_unavailable.insert(key);
                if let Some(count) = event_unavailable_causes.get_mut(&error.id) {
                    *count += 1;
                } else {
                    event_unavailable_causes.insert(error.id.clone(), 1);
                }
                event_definitions.mark_unavailable(key, id, flags, error);
            }
        }
    }
    // initial-classicmangos.sql:148741-160209,450914-451331.
    assert_eq!(event_census, [11277, 0, 192]);
    assert_eq!(event_unavailable.len(), 123);
    assert_eq!(
        event_unavailable_causes,
        BTreeMap::from([
            (
                "execution.event_ai.loader.effective_quest_flags_owner".into(),
                9
            ),
            ("execution.event_ai.loader.relay_store_owner".into(), 151),
            ("execution.event_ai.loader.relay_template_owner".into(), 7),
            ("execution.event_ai.loader.summon_store_owner".into(), 25),
        ])
    );
    let event_definitions = std::sync::Arc::new(event_definitions);
    let summon_query = observed.event_ai_summon_query.unwrap();
    assert!(template_query_line < summon_query.0);
    assert!(summon_query.0 < event_query.0);
    println!(
        "F6_EVENT_AI_STORE loaded={} skipped={} refused={} unavailable_keys={} matched_loader_sql=1 matched_random_template_sql=1 unmatched_summon_sql=1 query_line={} random_template_request_line={template_query_line} summon_observed_only_line={}",
        event_census[0], event_census[1], event_census[2], event_unavailable.len(),
        event_query.0, summon_query.0
    );
    println!(
        "F6_EVENT_AI_LOAD_DISPOSITIONS cpp_skipped={event_skipped_sites:?} unrepresented={event_unavailable_causes:?}"
    );
    for (entry, guid) in [(687, 1996), (4692, 28044)] {
        println!(
            "F6_EVENT_AI_AVAILABILITY entry={entry} guid={guid} result={:?}",
            event_definitions.check_spawn_availability(entry, guid, false)
        );
    }
    // World.cpp:1316 loads EventAI before ScriptDevAIMgr at :1333.
    assert!(event_query.0 < observed.script_library_query_line.unwrap());
    assert!(observed
        .group_queries
        .iter()
        .all(|(line, _)| *line < event_query.0));
    let collision_models = SourceCollisionModels::load(catalog).unwrap();
    // World.cpp:927,1333,1341 precedes CreateContinents at :1389.
    let scripts = std::mem::take(&mut observed.script_registry).load(
        catalog,
        &creature_templates,
        &gameobject_templates,
    );
    struct AddonSpellOwners;
    impl SpellLoaderOwner for AddonSpellOwners {}
    let addon_spells = SourceSpellStores::load(catalog, &mut AddonSpellOwners).unwrap();
    let filtered =
        super::super::spawn_loader_config::assignment(&config, "LogFilter_DbStrictedCheck")
            .unwrap()
            .1;
    let mut addon_owner = CreatureAddonCatalog::new(catalog);
    addon_owner.templates = Some(&creature_templates);
    addon_owner.spawns = Some(spawns);
    addon_owner.spells = Some(&addon_spells);
    addon_owner.strict_check_filtered = Some(
        filtered == "1"
            || filtered.eq_ignore_ascii_case("true")
            || filtered.eq_ignore_ascii_case("yes"),
    );
    let creature_addons = LoadedCreatureAddons::load(&mut addon_owner).unwrap();
    assert_eq!(creature_addons.record_counts, [7474, 1789]);
    assert!(creature_addons.notices.is_empty());
    let unterminated = [CreatureAddonTable::Template, CreatureAddonTable::Guid]
        .into_iter().flat_map(|table| creature_addons.table_rows(table))
        .filter(|row| matches!(&row.auras, CreatureAddonAuras::Spells(spells) if spells.last() != Some(&0)))
        .count();
    assert_eq!(unterminated, 0);
    println!("F6_ADDON_UB aura_terminator_out_of_bounds_stock_reach={unterminated}");
    spawn_init::bind_mmap_world(recording, &config, &mut geometry.mmap).unwrap();
    let mut corpses = active_init::frozen_corpses(recording).unwrap();
    println!(
        "F6_CORPSES loaded=0 matched_sql=1 query_line={}",
        observed.corpse_query_line.unwrap()
    );
    assert_eq!(
        observed
            .group_queries
            .iter()
            .map(|(_, query)| query.as_str())
            .collect::<Vec<_>>(),
        GROUP_QUERIES
    );
    println!(
        "F6_SPAWN_GROUP_SQL matched_sql=6 query_lines={:?}",
        observed.group_queries
    );
    let mut groups = SourceSpawnGroups::default();
    groups
        .load_effective_catalog(
            catalog,
            Path::new(&std::env::var_os("BENILLA_F2_GROUP_OCCURRENCES").unwrap()),
            spawns,
            super::super::spawn_loader_config::assignment(&config, "Autoload.Active")
                .unwrap()
                .1
                == "1",
        )
        .unwrap();
    equipment::census(
        &creature_templates,
        spawns,
        &spawn_templates,
        inputs,
        &groups,
    );
    formation_path::census(spawns, &groups);
    let script_tables = [DbScriptType::Relay, DbScriptType::CreatureMovement]
        .into_iter()
        .map(|kind| {
            (
                kind,
                catalog_db_script_rows(catalog, script_occurrences, kind).unwrap(),
            )
        })
        .collect();
    let db_scripts = SourceDbScriptMaps::load(
        script_tables,
        script_templates,
        &mut DbScriptCatalog::new(
            catalog,
            Some(&addon_spells),
            &mut groups,
            Some(
                filtered == "1"
                    || filtered.eq_ignore_ascii_case("true")
                    || filtered.eq_ignore_ascii_case("yes"),
            ),
        ),
    )
    .unwrap();
    let mut waypoint_trace = |_| {};
    let mut waypoint_random =
        SourceRandom::new(1, &mut *engine, &mut *cursor, &mut waypoint_trace).unwrap();
    let waypoint_paths = waypoints::load(
        catalog,
        spawns,
        &creature_templates,
        &db_scripts,
        geometry,
        &mut waypoint_random,
    )
    .unwrap();
    assert_eq!(waypoint_paths.report.statements, WAYPOINT_MANAGER_QUERIES);
    // Snapshot tables begin at :335034,388530,861610; primary-key census.
    assert_eq!(waypoint_paths.report.rows, [53469, 17123, 7752]);
    assert_eq!(waypoint_paths.report.nodes, [53469, 17123, 7752]);
    assert_eq!(waypoint_paths.report.paths, [2956, 619, 244]);
    assert_eq!(waypoint_paths.report.missing_owner, [0; 3]);
    assert_eq!(waypoint_paths.report.blacklisted_nodes, [0; 3]);
    assert_eq!(
        observed
            .waypoint_queries
            .iter()
            .map(|(_, q)| q.as_str())
            .collect::<Vec<_>>(),
        WAYPOINT_MANAGER_QUERIES,
    );
    assert_eq!(
        observed
            .waypoint_queries
            .iter()
            .map(|(line, _)| *line)
            .collect::<Vec<_>>(),
        [54691906, 54713296, 56487193, 56491349, 57128194, 57130068, 57131519],
    );
    println!(
        "F6_WAYPOINT_MANAGER_SQL matched_sql=7 lines={:?}",
        observed.waypoint_queries
    );
    waypoints::census(&waypoint_paths, spawns, &groups, catalog);
    let mut dynamic = BTreeMap::<u32, [Vec<u32>; 2]>::new();
    for ((map, kind), guids) in std::mem::take(&mut spawns.dynamic_guids) {
        dynamic.entry(map).or_default()[usize::from(kind == GridSpawnKind::GameObject)] = guids;
    }
    let receipt_bytes = std::fs::read(recording.join("actual-process-environment.json")).unwrap();
    let receipt: benilla_sim::source_sine::ProcessReceipt =
        serde_json::from_slice(&receipt_bytes).unwrap();
    assert_eq!(
        Sha256Id::of(&receipt_bytes),
        Sha256Id::parse("4b7d35a74a48e0829d942539b97f940f05b4d57416eae71a3caf4dd6269859b5")
            .unwrap()
    );
    for (file, expected) in [
        (
            "acquisition-input-manifest.json",
            &receipt.source_acquisition_bindings.manifest,
        ),
        (
            "trace-schema.json",
            &receipt.source_acquisition_bindings.schema,
        ),
        (
            "instrumentation-identity.json",
            &receipt.source_acquisition_bindings.instrumentation,
        ),
        (
            "run/trace.tsv.process-environment.bin",
            &receipt.source_process_receipt_sha256,
        ),
    ] {
        assert_eq!(
            Sha256Id::of(&std::fs::read(recording.join(file)).unwrap()),
            Sha256Id::parse(expected).unwrap()
        );
    }
    let dispatch = benilla_sim::source_sine::DispatchBinding::from_process_receipt(
        16,
        &receipt.source_acquisition_bindings,
        &receipt,
    )
    .unwrap();
    let trig_dispatch = benilla_sim::source_trig_fma::DispatchBinding::from_process_receipt(
        16,
        &receipt.source_acquisition_bindings,
        &receipt,
    )
    .unwrap();
    geometry
        .vmap
        .bind_rotation_dispatch(16, &receipt.source_acquisition_bindings, &receipt)
        .unwrap();
    let mut calendar = Calendar(std::mem::take(&mut observed.calendar));
    let mut lunar_trace = Vec::new();
    let lunar = ReferenceLunarEventOwner::new(
        &mut calendar,
        &mut lunar_trace,
        moon::SineOrder::SolarFirst,
        dispatch,
    );
    let ids = catalog.execution_event_reference_ids().unwrap();
    let quest_activity = SourceQuestActivity::from_quest_ids(ids[3].iter().copied());
    let quest_activity_at_load: BTreeMap<_, _> = quest_activity.activity().collect();
    assert!(quest_activity_at_load.keys().eq(ids[3].iter()));
    assert!(quest_activity_at_load.values().all(|active| *active));
    let assert_quest_activity = |owner: &Owner<'_>| {
        for (quest, active) in owner.quest_activity.activity() {
            assert_eq!(
                active,
                owner
                    .quests
                    .get(&quest)
                    .copied()
                    .unwrap_or(quest_activity_at_load[&quest]),
                "live activity for quest {quest}"
            );
        }
        assert_eq!(owner.missing_quest_activity_writes, 0);
    };
    let mut owner = Owner {
        spawns,
        maps: &inputs.maps,
        ids,
        lunar,
        clocks: observed.load_clocks,
        update: observed.update_clocks,
        checks: observed.check_clocks,
        clock_count: 0,
        pools,
        states: SourceManagers::default(),
        loaded_maps: BTreeMap::new(),
        effects: SourceEventEffects::new(dynamic),
        quests: BTreeMap::new(),
        quest_activity,
        missing_quest_activity_writes: 0,
        status: BTreeSet::new(),
        sql: Vec::new(),
        pool_operations: Vec::new(),
        observations: Vec::new(),
        link_errors: Vec::new(),
        engine,
        cursor,
        raw: Vec::new(),
    };
    let mut events = SourceGameEventManager::default();
    let mut pools = std::mem::take(&mut owner.pools);
    events
        .load_catalog_definitions(catalog, event_occurrences, &mut pools, &mut owner)
        .unwrap();
    owner.pools = pools;
    assert_eq!(&owner.quests, &events.inventory().quest_active);
    assert_quest_activity(&owner);
    assert!(owner.clocks.is_empty());
    assert!(events.inventory().loaded);
    // World.cpp:1389 precedes :1416. Loading event 7's calendar is not StartEvent.
    assert!(events.inventory().active.is_empty() && !events.inventory().initialized);
    let event_initialize_line = observed.event_initialize_line.unwrap();
    assert!(observed
        .event_status_writes
        .iter()
        .all(|(line, _)| *line > event_initialize_line));
    assert!(!observed
        .event_status_writes
        .iter()
        .any(|(_, sql)| sql == "INSERT INTO game_event_status (event) VALUES (7)"));
    assert!(!observed
        .event_status_writes
        .iter()
        .any(|(_, sql)| sql == "INSERT INTO game_event_status (event) VALUES (27)"));
    let mut event_load_counts = [0usize; 4];
    for observation in &owner.observations {
        match observation {
            GameEventObservation::Definition { .. } => event_load_counts[1] += 1,
            GameEventObservation::Member { kind, .. } => {
                event_load_counts[2 + usize::from(*kind == EventSpawnKind::GameObject)] += 1;
            }
            GameEventObservation::Decision { slot: 1, .. } => event_load_counts[1] += 1,
            GameEventObservation::Decision { slot: 3, .. } => event_load_counts[2] += 1,
            GameEventObservation::Decision { slot: 4, .. } => event_load_counts[3] += 1,
            _ => {}
        }
    }
    assert_eq!(event_load_counts[1..], [67, 3239, 11204]);
    let reset: serde_json::Value =
        serde_json::from_slice(&std::fs::read(recording.join("reset.json")).unwrap()).unwrap();
    let character = &reset["databases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["database"] == "characters")
        .unwrap();
    let snapshot = retained_f1_file(
        recording.ancestors().nth(3).unwrap(),
        Path::new(character["snapshot"].as_str().unwrap()),
        &Sha256Id::parse(character["sha256"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    for line in snapshot.split(|&b| b == b'\n') {
        if let Some(mut rest) = line.strip_prefix(b"INSERT INTO `game_event_status` VALUES ") {
            loop {
                if rest == b";" {
                    break;
                }
                assert_eq!(rest.first(), Some(&b'('));
                let (value, next) = super::super::manager_inputs::sql_value(&rest[1..]).unwrap();
                owner.status.insert(
                    std::str::from_utf8(value.as_ref().unwrap())
                        .unwrap()
                        .parse()
                        .unwrap(),
                );
                assert_eq!(next.first(), Some(&b')'));
                rest = &next[1..];
                if rest.first() == Some(&b',') {
                    rest = &rest[1..];
                }
            }
        }
    }
    let db: serde_json::Value = serde_json::from_slice(
        &std::fs::read(recording.join("db-proxy-clock-input.json")).unwrap(),
    )
    .unwrap();
    let mut query_clock = |slot: usize, table: &str| {
        let sql = format!("DELETE FROM {table} WHERE respawntime <= UNIX_TIMESTAMP(NOW())");
        let digest = Sha256Id::of(sql.as_bytes());
        let clocks = db["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["statement_sha256"] == digest.as_str())
            .collect::<Vec<_>>();
        assert_eq!(clocks.len(), 1);
        CharacterQueryOccurrence {
            owner: observed.persistent_owners[slot].take().unwrap(),
            database_seconds: clocks[0]["microseconds"].as_i64().unwrap() / 1_000_000,
            world_seconds: observed.world_seconds.unwrap(),
            ordinals: Vec::new(),
        }
    };
    let initial = ManagerInitialInputs::frozen_snapshot(
        &snapshot,
        [
            query_clock(0, "creature_respawn"),
            query_clock(1, "gameobject_respawn"),
        ],
    )
    .unwrap();
    let mut persistent = InitialEffects {
        initial: &initial,
        operations: Vec::new(),
        cleaned: [false; 2],
        outbound: Vec::new(),
    };
    owner
        .states
        .initialize_persistent(
            &initial,
            &mut Definitions {
                spawns,
                maps: &inputs.maps,
            },
            &mut persistent,
        )
        .unwrap();
    // World.cpp:1134/1137 respawns precede :1158; map jobs start only at :1389.
    // These bounded diagnostics do not claim to port intervening World loaders.
    let graveyard_cursor = *owner.cursor;
    // World.cpp:1114 precedes graveyards; :1208 precedes map construction.
    let (dungeon_store, encounter_rows, encounter_references) =
        catalog.execution_encounter_operands().unwrap();
    assert_eq!(
        observed
            .encounter_queries
            .iter()
            .map(|(_, query)| query.as_str())
            .collect::<Vec<_>>(),
        ENCOUNTER_QUERIES
    );
    let expected_max = dungeon_store
        .rows()
        .keys()
        .next_back()
        .unwrap()
        .to_string()
        .into_bytes();
    assert_eq!(
        observed.encounter_sql[ENCOUNTER_QUERIES[0]],
        vec![vec![expected_max]]
    );
    assert_eq!(
        observed.encounter_sql[ENCOUNTER_QUERIES[1]],
        vec![vec![dungeon_store.rows().len().to_string().into_bytes()]]
    );
    let expected_dungeon = dungeon_store
        .rows()
        .values()
        .map(|entry| {
            let mut row = [
                entry.id,
                entry.map,
                entry.difficulty,
                entry.data,
                entry.index,
                entry.complete_world_state,
            ]
            .into_iter()
            .map(|value| value.to_string().into_bytes())
            .collect::<Vec<_>>();
            row.extend(entry.names.iter().cloned());
            row.extend([entry.name_flags, entry.icon].map(|value| value.to_string().into_bytes()));
            row
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed.encounter_sql[ENCOUNTER_QUERIES[2]],
        expected_dungeon
    );
    assert_eq!(
        observed.encounter_sql[ENCOUNTER_QUERIES[3]],
        encounter_rows
            .iter()
            .map(|row| row
                .iter()
                .map(|value| value.to_string().into_bytes())
                .collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
    assert!(observed
        .encounter_raw
        .values()
        .all(|(start, end)| start == end));
    assert_eq!(*owner.cursor, graveyard_cursor);
    let (graveyard_rows, graveyard_references) = catalog.execution_graveyard_operands().unwrap();
    assert_eq!(graveyard_rows, observed.graveyard_rows);
    assert_eq!(
        graveyard_rows
            .iter()
            .map(|row| [row[0], row[1], row[2]])
            .collect::<BTreeSet<_>>()
            .len(),
        190,
    );
    let mut world_graveyards = SourceGraveyardManager::default();
    let graveyard_census =
        world_graveyards.load_zones(Some(&graveyard_rows), &graveyard_references);
    assert_eq!(
        (graveyard_census.visited, graveyard_census.inserted),
        (190, 190)
    );
    assert!(graveyard_census.skipped.is_empty());
    assert_eq!(*owner.cursor, graveyard_cursor);
    assert_eq!(observed.graveyard_query_line, Some(46242412));
    assert_eq!(observed.graveyard_last_field_line, Some(46245670));
    assert_eq!(observed.graveyard_raw_start, observed.graveyard_raw_end);
    assert_eq!(observed.graveyard_raw_start, Some(owner.cursor.raw));
    println!(
        "F6_GRAVEYARDS query_line={} last_field_line={} rows={} loaded={} skipped={:?} additional_raw_words=0",
        observed.graveyard_query_line.unwrap(),
        observed.graveyard_last_field_line.unwrap(),
        graveyard_census.visited,
        graveyard_census.inserted,
        graveyard_census.skipped
    );
    let (encounters, encounter_census) = LoadedInstanceEncounters::load(
        None,
        Some(&encounter_rows),
        &dungeon_store,
        &encounter_references,
    );
    assert_eq!(
        encounters.shift_census().1,
        0,
        "stock reaches signed-shift UB; stop before committing"
    );
    println!(
        "F6_ENCOUNTERS dungeon={} visited={} loaded={} skipped={:?} shift_census={:?} matched_sql=4 additional_raw_words=0 query_lines={:?}",
        dungeon_store.rows().len(),
        encounter_census.visited,
        encounter_census.loaded,
        encounter_census.skipped,
        encounters.shift_census(),
        observed.encounter_queries
    );
    assert!(observed.encounter_queries[2].0 < observed.graveyard_query_line.unwrap());
    assert!(observed.graveyard_last_field_line.unwrap() < observed.encounter_queries[3].0);
    let mut outdoor_pvp = SourceOutdoorPvp::default();
    let mut outdoor_messages = SourceMessager::new(MessageOwner::World);
    // The current registry is empty before MapManager::Initialize, not a recorded result.
    assert!(owner.loaded_maps.is_empty());
    let mut current_maps = owner
        .loaded_maps
        .values()
        .map(|&map| (map, SourceMessager::new(MessageOwner::Map(map))))
        .collect();
    let outdoor_census = outdoor_pvp
        .initialize(
            SourceOutdoorPvp::config(&config).unwrap(),
            1,
            &mut outdoor_messages,
            &mut current_maps,
        )
        .unwrap();
    assert_eq!(outdoor_census.maps_visited, 0);
    assert_eq!(outdoor_census.scripts, 2);
    assert_eq!(outdoor_messages.pending(), 1);
    println!(
        "F6_OUTDOOR_PVP scripts={} current_maps_visited={} world_messages_queued={} graveyard_team_before_world_drain={:?}",
        outdoor_census.scripts,
        outdoor_census.maps_visited,
        outdoor_messages.pending(),
        world_graveyards.entries().get(&139)
    );
    let continent_config = ContinentConfig::parse(&config).unwrap();
    let definitions = ContinentDefinitions::load(catalog, &inputs.maps, &config).unwrap();
    // Map.h:201 reads the loaded Map.dbc entry for PathFinder.cpp:395.
    geometry.bind_pathfinder_maps(catalog).unwrap();
    let temporary_starts = definitions.temporary_starts;
    let script_ids = &definitions.script_ids;
    let script_names = &definitions.script_names;
    let mut ignore_constructor_trace = |_| {};
    let mut random = SourceRandom::new(
        1,
        &mut *owner.engine,
        &mut *owner.cursor,
        &mut ignore_constructor_trace,
    )
    .unwrap();
    let mut continents = SourceContinents::construct(
        definitions.entries,
        GuidMaximum::Value(initial.maximum_instance_id()),
        geometry,
        &continent_config,
        &mut random,
    )
    .unwrap();
    assert_eq!(random.cursor().raw, 8712); // Both TerrainInfo objects already exist.
    assert_eq!(continents.max_instance_id, 0); // MAX(id) is NULL at trace:60100919.
    for map in &mut continents.maps {
        map.creature_spells = Some(CreatureSpellMapData(std::sync::Arc::clone(
            &creature_spell_lists,
        )));
        assert!(owner
            .loaded_maps
            .insert((map.identity.map, 0), map.identity)
            .is_none());
    }
    assert_eq!(owner.loaded_maps.len(), observed.map_births);
    assert_eq!(observed.map_births, 2);
    assert_eq!(observed.map_bindings[&0], (6, 4294967667, 60101188));
    assert_eq!(observed.map_bindings[&1], (7, 4294967669, 60117267));
    assert_eq!(observed.map_identities[&4294967667].0, 60100929);
    assert_eq!(observed.map_identities[&4294967669].0, 60100933);
    // Execute each job's local prelude independently. No cross-thread order is claimed.
    for map in &mut continents.maps {
        map.initialize_prelude(temporary_starts, geometry, &continent_config)
            .unwrap();
        assert!(map.prelude_complete());
        assert_eq!(map.visibility.to_bits(), 100.0f32.to_bits());
        assert_eq!(geometry.terrain(map.identity.map).unwrap().references(), 1);
        assert!(map.persistent.is_none() && map.variables.is_empty() && !map.spawn_updated);
        for y in 0..64 {
            for x in 0..64 {
                assert!(!map.grid_loaded(x, y).unwrap());
                assert!(map.cells.grids.identity(x, y).is_none());
            }
        }
    }
    assert_eq!(
        observed.map_threads[&6].before_sql,
        [(60100952, 0, 2440933613), (60100962, 1, 3174165029)]
    );
    assert!(observed.map_threads[&7].before_sql.is_empty());
    assert_eq!(observed.map_threads[&6].sql_line, Some(60100980));
    assert_eq!(observed.map_threads[&7].sql_line, Some(60101153));
    let mut links = SourceCreatureLinking::default();
    let occurrences = std::env::var_os("BENILLA_F2_GROUP_OCCURRENCES").unwrap();
    let mut rows = catalog
        .execution_linking_rows(Path::new(&occurrences))
        .unwrap();
    links
        .load(
            &mut rows,
            &mut LinkingDefinitions {
                catalog,
                spawns: owner.spawns,
                inputs,
            },
        )
        .unwrap();
    let objects = SourceObjects::default();
    let mut matched_map_words = [0u64; 2];
    let mut matched_map_sql = 0;
    let world_clock = SourceWorldClock::default();
    assert_eq!(world_clock.current_ms(), 0);
    for (slot, thread, first_line, first_word) in
        [(0, 6, 60100952, 2440933613), (1, 7, 60117712, 159038541)]
    {
        let evidence = &observed.map_threads[&thread];
        let stream = Mt19937::from_state(evidence.initial.clone().unwrap()).unwrap();
        // MoveSplineInit.cpp:27 initializes this OS thread's counter beside its RNG.
        let mut spline_thread = SourceSplineThread::new(thread);
        let mut cursor = DrawCursor::default();
        let mut trace = Vec::new();
        let mut sink = |event| trace.push(event);
        let mut engine = stream;
        let mut random = SourceRandom::new(thread, &mut engine, &mut cursor, &mut sink).unwrap();
        let script_id = script_ids[&(slot as u32)];
        assert_ne!(script_id, 0);
        let mut map_effects = InitialEffects {
            initial: &initial,
            operations: Vec::new(),
            cleaned: [false; 2],
            outbound: Vec::new(),
        };
        let mut storage = SnapshotWorldStorage {
            initial: &initial,
            effects: &mut map_effects,
            pending: BTreeSet::new(),
            issued: Vec::new(),
        };
        continents.maps[slot]
            .create_instance_data(
                script_id,
                &script_names[&(slot as u32)],
                true,
                &mut random,
                &mut storage,
            )
            .unwrap();
        assert_eq!(random.cursor().raw, [2, 0][slot]);
        assert_eq!(storage.issued.len(), 2);
        for (statement, recorded) in storage.issued.iter().zip(&evidence.sql_statements) {
            assert_eq!(statement.as_bytes(), recorded);
        }
        assert_eq!(evidence.sql_statements.len(), 2);
        if slot == 1 {
            assert_eq!(evidence.sql_lines.len(), storage.issued.len());
            assert_eq!(evidence.sql_lines, [60101153, 60113864]);
        }
        matched_map_sql += 2;
        assert!(storage.pending.contains(&(slot as u32)));
        assert_eq!(
            initial.world_data(slot as u32).unwrap(),
            WorldInstanceRow::Missing
        );
        assert_eq!(
            map_effects.outbound,
            [PersistentSql::CreateWorld { map: slot as u32 }]
        );
        assert!(continents.maps[slot].instance_data.is_some());
        assert!(continents.maps[slot].persistent.is_none() && !continents.maps[slot].spawn_updated);
        if slot == 0 {
            assert_eq!(
                continents.maps[slot].variables.values(),
                BTreeMap::from([(330001, 1), (330002, 1)])
            );
        } else {
            assert!(continents.maps[slot].variables.is_empty());
            assert_eq!(random.cursor().raw, 0);
        }
        assert!(evidence.pool_snapshot_line.unwrap() < event_initialize_line);
        // Only the loaded definitions are forked for two independent diagnostic prefixes.
        // A refused job's stopped flag must not invent a scheduling dependency on its peer.
        let mut pools = owner.pools.clone();
        let mut states = std::mem::take(&mut owner.states);
        continents.maps[slot]
            .attach_persistent_state(&mut states, &mut persistent)
            .unwrap();
        let shuffles = {
            let mut work = MapPoolWork {
                world: &mut owner,
                random: &mut random,
                map: &continents.maps[slot],
                links: &links,
                objects: &objects,
                shuffles: Vec::new(),
                persistent: Vec::new(),
            };
            continents.maps[slot]
                .initialize_pools(&mut states, &mut pools, &events, &mut work)
                .unwrap();
            work.shuffles
        };
        let after_pools_cursor = random.cursor().raw;
        continents.maps[slot]
            .initialize_after_pools(&states, &world_graveyards, Some(&encounters))
            .unwrap();
        let state = continents.maps[slot].persistent.unwrap();
        let mut spawn_manager =
            SourceSpawnManager::new(continents.maps[slot].identity, state).unwrap();
        let mut formations = SourceFormations::default();
        let clock = evidence.spawn_clock;
        let mut spawn_work = spawn_init::Initialize {
            thread,
            clock,
            spawns: owner.spawns,
            effects: &owner.effects,
            map: &continents.maps[slot],
            pools: &pools,
            links: &links,
            objects: &objects,
            groups: &mut groups,
            formations: &mut formations,
            operations: Vec::new(),
            persistent: Vec::new(),
            visits: [0; 2],
        };
        let spawn_result = spawn_manager.initialize(&mut states, &mut spawn_work);
        println!(
            "F6_SPAWN_INIT map={slot} clock={clock:?} dynamic={:?} result={:?} active={} grid_operations={} groups={}",
            spawn_work.visits,
            spawn_result,
            spawn_manager.inventory().active.len(),
            spawn_work.persistent.len(),
            spawn_manager.inventory().groups.len()
        );
        spawn_result.unwrap();
        assert_eq!(
            clock,
            Some((
                if slot == 0 { 60379860 } else { 60306683 },
                1794314190,
                after_pools_cursor as usize
            ))
        );
        assert!(spawn_work.clock.is_none());
        assert!(spawn_manager.inventory().initialized);
        assert_eq!(
            spawn_manager
                .inventory()
                .groups
                .keys()
                .copied()
                .collect::<BTreeSet<_>>(),
            evidence.spawn_groups
        );
        // Independent catalog 757dcfb8 census: map 0 has 433+12 creatures; map 1 has 2015.
        // Trace irand lines 37001780-37595457 resolve 19 mixed lists for comparison only.
        // All GO masks are 1. Reset cf538fd2 has no respawns; masters 44251/43201
        // have no links or pools, so CreatureLinkingMgr.cpp:663-672,699-732 allows all.
        let expected_dynamic = if slot == 0 { [445, 41] } else { [2015, 41] };
        assert_eq!(spawn_work.visits, expected_dynamic);
        assert!(spawn_manager.inventory().active.is_empty());
        let overlays =
            [GridSpawnKind::Creature, GridSpawnKind::GameObject].map(|wanted| {
                spawn_work.persistent.iter().filter(|operation| matches!(operation,
                PersistentOperation::Overlay { kind, insert: true, .. } if *kind == wanted
            )).count()
            });
        assert_eq!(overlays, expected_dynamic);
        assert_eq!(
            spawn_work.persistent.len(),
            expected_dynamic.iter().sum::<usize>()
        );
        geometry
            .mmap
            .initialize_map(continents.maps[slot].identity)
            .unwrap();
        assert!(geometry.mmap.enabled());
        assert_eq!(
            super::super::spawn_loader_config::assignment(&config, "mmap.preload")
                .unwrap()
                .1,
            "0"
        );
        let query = geometry.mmap.query(slot as u32, 0).unwrap();
        assert_eq!(
            geometry.mmap.query_capacity(slot as u32, &query).unwrap(),
            1024
        );
        assert_eq!(
            evidence.first_mmap_query,
            Some((if slot == 0 { 60382080 } else { 60310114 }, 1024))
        );
        let mut active_objects = SourceObjects::default();
        let pathfinders_before = geometry.pathfinder_constructor_completions;
        let calculations_before = geometry.pathfinder_calculate_completions;
        let mut active_clocks = evidence.active_clocks.clone();
        let active_init::ActivePrefixReport {
            refusal,
            link_follow,
            completed_objects,
            first_go,
            first_creature,
            mut attempt,
            candidates,
            mut candidate,
            mut entry,
            mut lookups,
            mut init_entry_fields,
            mut bound_guid,
            mut go_load_operands,
            mut go_factory_elevator,
            mut go_animprogress,
            mut creature_display_chance,
            mut creature_model,
            mut creature_entry,
            equipment_writes,
            swimming_census,
            gameobjects,
            completed,
            mut go_battleground,
            mut go_zone,
            mut go_script_id,
            mut visibility,
            mut creature_battleground,
            mut creature_zone,
            mut creature_linking_trigger,
            zone_lookups,
            mut selected_ai,
            units,
            motions,
            ais,
            event_ais,
        } = active_init::load_first(
            &mut continents.maps[slot],
            owner.spawns,
            inputs,
            &events,
            &groups,
            waypoint_paths.clone(),
            &creature_templates,
            &creature_addons,
            &addon_spells,
            &mut active_clocks,
            &world_clock,
            &gameobject_templates,
            &scripts,
            &spawn_templates,
            &creature_models,
            &equipment,
            &creature_immunities,
            &collision_models,
            &mut random,
            &mut spline_thread,
            &mut dynamic_models,
            trig_dispatch,
            &level_definitions,
            &level_config,
            &encounters,
            &faction_flags,
            &ai_registry,
            &ai_factions,
            &ai_templates,
            &ai_config,
            event_definitions.clone(),
            &outdoor_pvp,
            &links,
            &pools,
            &mut states,
            geometry,
            &mut active_objects,
            &mut corpses,
            thread,
            &config,
        )
        .unwrap();
        for &handle in completed_objects.iter().chain(std::iter::once(&candidate)) {
            assert_eq!(
                continents.maps[slot].passengers.transport(handle).unwrap(),
                None
            );
            // Object.cpp:105-108,115-118. The DB creature path has no Set*Guid writer.
            let object = active_objects.get(handle).unwrap();
            if object.kind == SourceObjectKind::Creature {
                let live = units.get(&active_objects, handle).unwrap();
                assert_eq!(
                    live.control_guids,
                    SourceControlGuids::default(),
                    "{}",
                    object.database_guid
                );
                for master in live
                    .spell_starts
                    .spells
                    .values()
                    .filter_map(|spell| spell.creature_cooldown.master)
                {
                    assert_eq!(
                        master,
                        SourceMasterLookup {
                            charmer: SourceControlLookup::EmptyGuid,
                            creator: Some(SourceControlLookup::EmptyGuid),
                        },
                        "Creature.cpp:3142, GUID {}",
                        object.database_guid
                    );
                }
            }
        }
        // Map 1's two initial and five later calculations precede the linked follow.
        if slot == 1 {
            assert_eq!(
                evidence.path_calculations,
                [
                    60367004, 60368544, 60449019, 60506680, 60509749, 60532760, 60533794,
                    // The three later path calculations all belong to GUID35138.
                    60580464, 60582481, 60583864,
                    // Spirit of Azuregos and Azuregos each build two waypoint paths.
                    60602723, 60603889, 60617339, 60618577,
                    // Nelson's first waypoint starts at :60651302.
                    60651302, 60653716,
                ]
            );
        }
        if slot == 0 {
            assert_eq!(
                evidence.path_calculations[..17],
                [
                    60489268, 60491574, 60495870, 60499147, 60500803, 60502330, 60504017, 60506382,
                    60560688, 60562420, 60569803, 60601394, 60617030, 60619528, 60621889, 60627229,
                    60629698,
                ]
            );
            assert_eq!(&evidence.path_calculations[17..19], [60645023, 60646526]);
            assert_eq!(
                &evidence.path_calculations[19..24],
                [60719961, 60752753, 60755748, 60766051, 60771421]
            );
            let paths = &evidence.path_calculations[24..];
            assert_eq!((paths.len(), paths[0], paths[21]), (22, 60833912, 60976274));
            assert_eq!(
                map0_receipt(paths.iter().map(|&line| [line])),
                "45d4b5cfa3064d661c4be4fb00820cadcb94f8c0120a2a33ade64979c51d2daa"
            );
            assert_eq!(evidence.sql_lines, [60100980, 60101139]);
            assert_eq!(
                Sha256Id::of(&std::fs::read(script_report).unwrap()),
                script_report_hash,
                "both ScriptDev registry views use BENILLA_F5_SCRIPT_REPORT"
            );
            assert_map0_continuation(evidence, &active_clocks);
            assert!(completed_objects[253..]
                .iter()
                .map(|&object| active_objects.get(object).unwrap().database_guid)
                .eq(evidence.continuation_completed[166..]
                    .iter()
                    .map(|&(_, guid)| guid)));
            let [holder] = event_ais[&completed_objects[253]].holders.as_slice() else {
                panic!("GUID4195 must retain event 26501");
            };
            // CreatureEventAI.cpp:40, trace :60711958-60711962.
            assert_eq!(
                (holder.event.id, holder.timer, holder.enabled),
                (26501, 13893, true)
            );
            assert_eq!(holder.event.actions[0].params, Some([0, 0, 12]));
            assert_eq!(
                completed_objects[137..143]
                    .iter()
                    .map(|&object| active_objects.get(object).unwrap().database_guid)
                    .collect::<Vec<_>>(),
                [4242, 4244, 5015, 15029, 15033, 4198]
            );
            let guard = completed_objects[137];
            assert_eq!(
                active_objects.get(guard).unwrap().guid.unwrap().entry(),
                3137
            );
            assert!(units.get(&active_objects, guard).unwrap().has_ai);
            let guard_ai = ais.get(&active_objects, guard).unwrap();
            assert!(guard_ai.guard);
            assert!(matches!(
                &guard_ai.loaded_class,
                LoadedCreatureAiClass::Builtin(executable)
                    if executable.builtin_kind() == Some(BuiltinCreatureAi::Guard)
            ));
            assert_eq!(
                guard_ai.base.generic_timers.map(|timer| (
                    timer.id,
                    timer.remaining,
                    timer.disabled
                )),
                [(2000, 0, true), (2001, 0, true)]
            );
            assert!(lookups.windows(5).any(|steps| steps
                == [
                    "creature_guard_ai_constructed",
                    "creature_ai_spell_list_changed",
                    "creature_ai_just_respawned",
                    "creature_instance_respawn_callback",
                    "creature_creator_loot_checked",
                ]));
            // Script::GetAI :60580449, SpellListChanged :60580583, JustRespawned
            // :60580604, OnCreatureRespawn :60580659, load completion :60580671.
            let chicken = completed_objects[143];
            let object = active_objects.get(chicken).unwrap();
            assert_eq!(
                (object.database_guid, object.guid.unwrap().entry()),
                (4228, 620)
            );
            assert!(object.in_world);
            // initial-classicmangos.sql:421539, Creature.cpp:541-559, npcs_special.cpp:68-71.
            let template = creature_templates.get(620).unwrap();
            assert_eq!((template.words[13], template.words[20]), (31, 2));
            let live = units.get(&active_objects, chicken).unwrap();
            assert_eq!((live.faction, live.loaded_creature.npc_flags), (31, 0));
            assert!(live.has_ai);
            // The recording does not instrument the field mask or Map::AddUpdateObject.
            assert!(!object.field_changed(UNIT_FIELD_FACTIONTEMPLATE).unwrap());
            assert!(object.field_changed(UNIT_NPC_FLAGS).unwrap());
            assert!(active_objects.client_update_pending(continents.maps[slot].identity, chicken));
            let ai = ais.get(&active_objects, chicken).unwrap();
            let LoadedCreatureAiClass::Scripted(script) = &ai.loaded_class else {
                panic!("entry 620 must retain the ScriptDev AI");
            };
            let SourceScriptedAi::ChickenCluck(state) = script else {
                panic!("expected ChickenCluck, got {script:?}");
            };
            assert_eq!(
                (script.binding().script_id, script.binding().name.as_str()),
                (261, "npc_chicken_cluck")
            );
            assert_eq!(
                (state.evade_check_cooldown_ms, state.reset_flag_timer_ms),
                (2500, 20000)
            );
            assert_eq!(
                ai.base
                    .generic_timers
                    .map(|timer| (timer.id, timer.remaining, timer.disabled)),
                [(2000, 0, true), (2001, 0, true)]
            );
            assert!(lookups.windows(13).any(|steps| steps
                == [
                    "creature_ai_script_registry_lookup",
                    "creature_ai_selected",
                    "creature_base_ai_constructed",
                    "creature_scripted_ai_constructed",
                    "creature_ai_spell_list_changed",
                    "creature_ai_just_respawned",
                    "creature_instance_respawn_callback",
                    "creature_creator_loot_checked",
                    "creature_aim_initialize_return",
                    "creature_spawn_relay_checked",
                    "creature_respawn_group_checked",
                    "creature_scripted_ai_retained",
                    "creature_load_from_db_return",
                ]));
            assert_eq!(
                evidence.bell_trace,
                [
                    60637563, 60637582, 60637844, 60637854, 60638050, 60638060, 60638066, 60638080,
                    60638101, 60638112,
                ]
            );
            let bell = completed_objects[189];
            let object = active_objects.get(bell).unwrap();
            let go = gameobjects.get(bell).unwrap();
            let ai = go.ai.as_ref().unwrap();
            assert_eq!((object.database_guid, go.template.entry), (15508, 175885));
            assert_eq!(
                (ai.object, ai.script),
                (bell, gameobjects::GameObjectScript::Bells)
            );
            assert_eq!(
                (
                    ai.bell_tolls,
                    ai.bell_sound,
                    ai.bell_timer,
                    ai.bell_plays_to_zone
                ),
                (0, 6675, 0, false)
            );
            assert!(object.in_world && object.active && object.notify_on_event);
            assert!(continents.maps[slot].cells.active().contains(bell));
            assert!(active_objects.event_notified(continents.maps[slot].identity, bell));
            assert!(lookups.windows(11).any(|steps| steps
                == [
                    "go15508_ai_initialize",
                    "go_ai_script_factory_lookup",
                    "go_ai_gameobject_info",
                    "go_ai_script_registry_lookup",
                    "go_ai_area",
                    "go_ai_area",
                    "go_ai_active_inserted",
                    "go_ai_factory_returned",
                    "go_ai_just_spawned",
                    "go_instance_data",
                    "go_instance_on_object_spawn",
                ]));
            assert!(!completed_objects.contains(&candidate));
            // GUID12058 casts Defensive Stance (SQL :152778, :843544).
            assert_eq!(
                (
                    attempt.guid,
                    attempt.kind,
                    entry,
                    attempt.cell.x(),
                    attempt.cell.y()
                ),
                (12058, GridSpawnKind::Creature, 4481, 480, 427)
            );
            assert!(active_objects.get(candidate).unwrap().in_world); // Trace :60983776.
            let live = units.get(&active_objects, candidate).unwrap();
            // SQL :425005, Creature.cpp:371-374. This warrior already uses rage.
            assert_eq!((live.unit_class, live.power_type), (Some(1), Some(1)));
            assert_eq!(live.shapeshift_form, 18);
            let stance = &live.spell_starts.spells[&1];
            let boost = &live.spell_starts.spells[&2];
            assert_eq!((stance.spell, boost.spell), (7164, 7376));
            assert_eq!(boost.triggered_by_aura, Some(7164));
            assert_eq!(boost.state, SourceSpellState::Landing);
            let queue = live.unit_events.inventory();
            // Spell.cpp:3008-3009; trace :60983998 and :60984600.
            assert!(queue
                .queued
                .iter()
                .filter_map(|event| {
                    if let SourceUnitEvent::Spell { spell } = event.kind {
                        Some((spell, event.event_id, event.exec_time_ms))
                    } else {
                        None
                    }
                })
                .eq([
                    (stance.id, stance.event.unwrap(), 1),
                    (boost.id, boost.event.unwrap(), 1),
                ]));
            // SpellAuras.cpp:148,140; the three boost effects bind before dispatch.
            let holder = live.auras.first_aura_holder(7376).unwrap();
            assert!(!holder.ready);
            assert!(holder
                .effects
                .iter()
                .flatten()
                .map(|aura| (aura.kind, aura.applied))
                .eq([(87, true), (79, false), (10, false)]));
            assert_eq!(refusal.id, active_init::CREATURE_NEXT_OWNER[0]);
        }
        assert_eq!(
            geometry.pathfinder_calculate_completions - calculations_before,
            evidence.path_calculations.len() as u64,
            "map={slot} next_owner={}",
            refusal.id
        );
        println!(
            "F6_NEXT_CANDIDATE map={slot} guid={} kind={:?} entry={entry} allocations={candidates} completed={completed} active_members={} refusal={} lookups={lookups:?}",
            attempt.guid, attempt.kind, continents.maps[slot].cells.active().len(), refusal.id
        );
        if slot == 1 {
            relations_evidence::assert_world_boss_reach(&creature_templates, owner.spawns);
            relations_evidence::assert_static_area_aura_reach(&creature_addons, &addon_spells);
            relations_evidence::assert_continuation_words(&evidence.words);
            relations_evidence::assert_continuation_clocks(&evidence.active_clocks, &active_clocks);
            assert_eq!(evidence.continuation_births.len(), candidates);
            assert_eq!(
                &evidence.continuation_births[201..223],
                [
                    60533943, 60534459, 60535154, 60535470, 60540348, 60541927, 60542125, 60542333,
                    60542568, 60543282, 60543408, 60543648, 60544248, 60544807, 60545897, 60547307,
                    60548598, 60549947, 60551003, 60551767, 60552415, 60553169,
                ]
            );
            assert_eq!(
                &evidence.continuation_completed[95..116],
                [
                    (60533941, 29113),
                    (60534450, 590005),
                    (60535151, 50942),
                    (60535454, 50950),
                    (60540316, 35936),
                    (60541913, 35983),
                    (60542307, 48398),
                    (60542561, 19225),
                    (60543279, 35143),
                    (60543405, 16112),
                    (60543644, 48469),
                    (60544246, 35161),
                    (60544805, 35157),
                    (60545883, 35108),
                    (60547301, 36005),
                    (60548591, 36307),
                    (60549927, 35785),
                    (60550990, 35132),
                    (60551761, 35888),
                    (60552409, 35890),
                    (60553163, 35884),
                ]
            );
            assert_eq!(completed_objects.len(), 281); // Trace :60649319.
            assert_eq!(evidence.sql_lines, [60101153, 60113864]);
            relations_evidence::assert_after_area_cast(
                &evidence.continuation_births,
                &evidence.continuation_completed,
            );
            for (object, (_, guid)) in completed_objects[218..]
                .iter()
                .zip(&evidence.continuation_completed[116..])
            {
                assert_eq!(active_objects.get(*object).unwrap().database_guid, *guid);
            }

            // AddToWorld at :60553931 and completion :60554098 follow the area cast.
            let area_owner = completed_objects[218];
            assert_eq!(active_objects.get(area_owner).unwrap().database_guid, 35891);
            assert!(active_objects.get(area_owner).unwrap().in_world);
            let area_unit = units.get(&active_objects, area_owner).unwrap();
            assert!(area_unit.has_ai && area_unit.loaded_creature.can_modify_stats);
            let cast = area_unit.spell_starts.spells.values().next().unwrap();
            assert_eq!((cast.spell, cast.state), (1032, SourceSpellState::Finished));
            assert_eq!(cast.damage_per_effect, [505, 0, 0]); // SQL:838502: 504 + 1.
            assert!(cast.spell_go.is_some() && cast.pending_aura.is_none());
            let target = cast.unique_targets.first().unwrap();
            assert_eq!(
                (target.unit, target.effect_mask, target.hit_mask),
                (area_owner, 1, 1)
            );
            assert!(target.processed && !target.missed);
            assert!(area_unit
                .unit_events
                .owns_spell_event(cast.event.unwrap(), cast.id));
            let (id, holder) = area_unit.auras.aura_holders_for_spell(1032).next().unwrap();
            assert_eq!((holder.caster, holder.target), (area_owner, area_owner));
            assert_eq!(
                (holder.duration, holder.diminish_group),
                (target.duration, 0)
            );
            assert_eq!(holder.apply_time, 1794314190); // Trace :60553739/:60553741.
            assert!(holder.ready && holder.skip_update && holder.permanent && !holder.deleted);
            let aura = holder.effects[0].as_ref().unwrap();
            assert_eq!(aura.apply_time, 1794314190); // Trace :60553823/:60553824.
            assert_eq!((aura.kind, aura.amount, aura.misc), (22, 505, 1)); // SQL:838502.
            assert!(aura.applied && aura.positive && !aura.periodic);
            assert_eq!(
                aura.area,
                Some(SourceAreaAura {
                    kind: SourceAreaAuraKind::Party,
                    radius_bits: 30f32.to_bits(), // SpellRadius.dbc: key 10, ordinal 3, column 1.
                    original_rank_spell: None,
                })
            );
            assert!(holder.effects[1..].iter().all(Option::is_none));
            assert_eq!(
                area_unit.auras.auras_of_type(22),
                [SourceAuraEffectRef {
                    holder: id,
                    effect: 0
                }]
            );
            // Computed armor: SQL:427381/:448365 gives 2255 armor and 65 agility.
            // ObjectMgr.cpp:782 and Creature.cpp:1331,1459 give base 2125.
            // StatSystem.cpp:664-672 adds 130 agility armor and SQL:838502 adds 505.
            assert_eq!(area_unit.resistances.armor(), 2760);

            // Spirit of Azuregos completes at :60608833 after Shroud of Death.
            let spirit = completed_objects[259];
            let spirit_object = active_objects.get(spirit).unwrap();
            assert_eq!(spirit_object.database_guid, 35867);
            assert!(spirit_object.in_world);
            let spirit = units.get(&active_objects, spirit).unwrap();
            assert!(spirit.has_ai && spirit.loaded_creature.can_modify_stats);
            assert!(spirit.auras.first_aura_holder(10848).unwrap().ready);

            // Trace :60616134-:60616151, :60623298 and SQL :426310 identify Azuregos.
            let azuregos = completed_objects[266];
            let azuregos_object = active_objects.get(azuregos).unwrap();
            assert_eq!(azuregos_object.database_guid, 52349);
            assert!(azuregos_object.in_world);
            assert_eq!(creature_templates.get(6109).unwrap().words[37], 3);
            let azuregos_unit = units.get(&active_objects, azuregos).unwrap();
            assert!(azuregos_unit.has_ai && azuregos_unit.loaded_creature.can_modify_stats);
            assert!(!motions[&azuregos].is_empty());
            let ai = ais.get(&active_objects, azuregos).unwrap();
            let LoadedCreatureAiClass::Scripted(SourceScriptedAi::Azuregos(_)) = &ai.loaded_class
            else {
                panic!("expected Azuregos, got {:?}", ai.loaded_class);
            };
            assert_eq!(ai.base.action_count(), 6);
            assert_eq!(azuregos_object.guid.unwrap().entry(), 6109);
            let cell = active_objects.viewpoint_cell(azuregos).unwrap().unwrap();
            assert_eq!((cell.x, cell.y), (590, 332));
            assert_eq!((cell.grid.x, cell.grid.y), (36, 20));
            assert!(lookups.windows(5).any(|steps| steps
                == [
                    "creature_scripted_ai_constructed",
                    "creature_ai_spell_list_changed",
                    "creature_ai_just_respawned",
                    "creature_instance_respawn_callback",
                    "creature_creator_loot_checked",
                ]));

            // GUID44205's terrain lookup at :60643213-:60643382 selects
            // zone 1377. The inherited empty callback precedes :60643388.
            let flayer = active_objects.get(completed_objects[276]).unwrap();
            assert_eq!(
                (flayer.database_guid, flayer.guid.unwrap().entry()),
                (44205, 11737)
            );
            assert_eq!(
                zone_lookups[&SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid: 44205
                }][0],
                1377
            );
            assert_eq!(
                lookups
                    .windows(4)
                    .filter(|steps| *steps
                        == [
                            "creature_outdoor_pvp_get_script",
                            "creature_silithus_pvp_on_create",
                            "creature_instance_data",
                            "creature_instance_on_creature_create",
                        ])
                    .count(),
                6
            );

            // Trace :60649357-:60649368 and :60657539 identify Nelson's unported factory.
            assert_eq!(
                (attempt.guid, attempt.kind, entry),
                (43224, GridSpawnKind::Creature, 14536)
            );
            assert_eq!((attempt.cell.x(), attempt.cell.y()), (273, 561));
            assert_eq!((attempt.grid.x, attempt.grid.y), (17, 35));
            assert!(!completed_objects.contains(&candidate));
            assert!(active_objects.get(candidate).unwrap().in_world);
            let stopped = units.get(&active_objects, candidate).unwrap();
            assert!(!stopped.has_ai && stopped.loaded_creature.can_modify_stats);
            assert!(!motions[&candidate].is_empty());
            let stat_aura = SpellTemplate {
                effect1: 6,
                effectapplyauraname1: 137,
                ..Default::default()
            };
            for unit in [spirit, azuregos_unit] {
                assert!(unit
                    .immunities
                    .immune_to_self_effect(&stat_aura, 0, &unit.auras)
                    .unwrap());
            }

            // Recording60539996 adds GUID35936 before its AI (60540194).
            let wavethrasher = completed_objects[201];
            assert_eq!(
                active_objects.get(wavethrasher).unwrap().database_guid,
                35936
            );
            assert!(active_objects.get(wavethrasher).unwrap().in_world);
            let live = units.get(&active_objects, wavethrasher).unwrap();
            // Recording60539793/60539826 has no VMap liquid. GridMap.cpp:1114-1124,1154-1157 uses the map tile.
            let f1 = recording
                .ancestors()
                .find(|path| path.ends_with("benilla-config/full-server/f1"))
                .unwrap();
            let tile = std::fs::read(f1.join("products/maps/0012743.map")).unwrap();
            assert_eq!(
                Sha256Id::of(&tile),
                Sha256Id::parse("e5b5306b69ca3433f03543fc0d423c7683b8686c9f25c3e4191ce1bdad703d0b")
                    .unwrap()
            );
            // Creature.cpp:441-442 sets MOVEFLAG_SWIMMING before SelectLevel (trace60539837).
            assert_ne!(
                live.movement.as_ref().unwrap().flags.unwrap() & 0x0020_0000,
                0
            );
            assert!(live.has_ai);
            assert!(!motions[&wavethrasher].is_empty());
            // SQL:435839 supplies addon7276. SpellAuras.cpp:3067-3082 adds no charges.
            let aura = live.auras.first_aura_holder(7276).unwrap();
            assert_eq!((aura.caster, aura.target), (wavethrasher, wavethrasher));
            assert!(aura.ready);
            assert_eq!(aura.charges, 0);
            assert_eq!(aura.apply_time, 1794314190); // Recording60539954.
            let effect = aura.effects[0].as_ref().unwrap();
            assert_eq!(
                (effect.kind, effect.applied, effect.apply_time),
                (42, true, 1794314190)
            ); // Recording60539964.
            let spell = live.spell_starts.spells.values().next().unwrap();
            assert_eq!(
                (spell.spell, spell.state),
                (7276, SourceSpellState::Finished)
            );
            assert_eq!(spell.proc_event_lookups, 1);
            assert_eq!(refusal.id, active_init::CREATURE_NEXT_OWNER[slot]);
        }
        println!("F6_WMO_CONTINUATION map={slot} raw={} pathfinder_constructors={} swimming={swimming_census:?} completed={completed} zone_lookups={}",
            random.cursor().raw, geometry.pathfinder_constructor_completions - pathfinders_before,
            zone_lookups.len());
        assert_eq!(
            continents.maps[slot].cells.active().len(),
            if slot == 0 { 5 } else { 7 }
        );
        let active_guids = continents.maps[slot]
            .cells
            .active()
            .members()
            .map(|handle| active_objects.get(handle).unwrap().database_guid)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            active_guids,
            if slot == 0 {
                // GUID4603 joins the four earlier members, trace :60825618/:60825623.
                BTreeSet::from([636, 2129, 4185, 4603, 15508])
            } else {
                BTreeSet::from([27279, 27280, 27281, 29113, 35867, 52349, 590005])
            }
        );
        {
            // Map.cpp:1581,1596; trace60348289/60348298 inserts twice, :60348296/:60348300 locks 1 then 2.
            assert_eq!(
                evidence
                    .active_adds
                    .iter()
                    .map(|(line, map, _, guid)| (*line, *map, *guid))
                    .collect::<Vec<_>>(),
                if slot == 0 {
                    vec![
                        (60403034, 0, 636),
                        (60403037, 0, 636),
                        (60488778, 0, 2129),
                        (60488782, 0, 2129),
                    ]
                } else {
                    vec![
                        (60348289, 1, 27280),
                        (60348298, 1, 27280),
                        (60355102, 1, 27279),
                        (60355106, 1, 27279),
                        (60355803, 1, 27281),
                        (60355807, 1, 27281),
                        (60532333, 1, 29113),
                        (60532335, 1, 29113),
                        (60534329, 1, 590005),
                        (60534331, 1, 590005),
                        (60602297, 1, 35867),
                        (60602299, 1, 35867),
                        (60616735, 1, 52349),
                        (60616739, 1, 52349),
                    ]
                }
            );
            assert_eq!(evidence.active_adds[0].2, evidence.active_adds[1].2);
            assert_eq!(
                evidence.active_locks,
                if slot == 0 {
                    vec![(60403035, 1), (60403039, 2), (60488780, 1), (60488783, 2)]
                } else {
                    vec![
                        (60348296, 1),
                        (60348300, 2),
                        (60355104, 3),
                        (60355108, 4),
                        (60355805, 5),
                        (60355809, 6),
                        (60532334, 1),
                        (60532336, 2),
                        (60534330, 3),
                        (60534332, 4),
                        (60602298, 1),
                        (60602300, 2),
                        (60616737, 3),
                        (60616741, 4),
                    ]
                }
            );
            let active = continents.maps[slot]
                .cells
                .active()
                .members()
                .next()
                .unwrap();
            let object = active_objects.get(active).unwrap();
            assert_eq!(object.database_guid, if slot == 0 { 636 } else { 27280 });
            assert!(object.active && object.in_world);
            assert_eq!(object.source_allocation, None);
            assert!(completed_objects.contains(&active));
            assert!(units.get(&active_objects, active).unwrap().has_ai);
            assert_eq!(
                continents.maps[slot]
                    .cells
                    .grids()
                    .active_lock_count(object.current_cell.unwrap().grid)
                    .unwrap(),
                if slot == 0 { 2 } else { 6 }
            );
        }
        println!("F6_EQUIPMENT_CONTINUATION map={slot} completed={completed} candidates={candidates} raw={} refusal={} current_equipment={:?}", random.cursor().raw, refusal.id, equipment_writes.get(&attempt.guid));
        if slot == 0 {
            // EventAI returns at :60478585; ObjectGridLoader.cpp:159 completes
            // GUID40520 at :60478609 before the next birth at :60478620.
            let candidate = completed_objects[87];
            let object = active_objects.get(candidate).unwrap();
            assert_eq!(
                (object.database_guid, object.guid.unwrap().entry()),
                (40520, 6491)
            );
            assert!(object.in_world);
            // Creature.cpp:2333-2343 applies 10848 after 9036, without a cast RNG draw.
            assert_eq!(
                creature_addons
                    .get(HighGuid::Creature, 40520, 6491)
                    .unwrap()
                    .auras,
                CreatureAddonAuras::Spells(vec![9036, 10848, 0])
            );
            assert_eq!(
                active_objects
                    .get(completed_objects[88])
                    .unwrap()
                    .database_guid,
                1578
            ); // :60479696
            let live = units.get(&active_objects, candidate).unwrap();
            let auras = &live.auras;
            assert_eq!(
                auras.aura_spell_ids().collect::<Vec<_>>(),
                [9036, 9036, 10848]
            );
            assert_eq!(auras.aura_type_ids().collect::<Vec<_>>(), [4, 95]);
            assert_eq!(auras.vis_flags, 1);
            assert_eq!(auras.cast_counter, 0);
            assert_eq!(live.spell_starts.spells.len(), 4);
            assert_eq!(motions[&candidate].current_type(), 0);
            assert!(!live.control_guids.has_charmer());
            let ai = &event_ais[&candidate];
            assert_eq!(ai.depth, 0);
            assert!(ai.staged[0].is_empty());
            let holder = &ai.holders[0];
            assert!(!holder.in_progress && holder.enabled);
            // acid_classic.sql:19391; CreatureEventAI.cpp:607-620,782-789.
            assert_eq!(
                (
                    holder.event.id,
                    holder.event.kind,
                    holder.event.chance,
                    holder.event.flags
                ),
                (649101, 11, 100, 0)
            );
            let first_action = &holder.event.actions[0];
            assert_eq!(
                (first_action.kind, first_action.params),
                (11, Some([10848, 0, 0]))
            );
            assert_eq!(
                (holder.event.actions[1].kind, holder.event.actions[1].params),
                (11, Some([9036, 0, 0]))
            );
            assert_eq!(holder.event.actions[2].kind, 0);
            // CreatureEventAI.cpp:789-804 reaches UnitAI.cpp:175-223 and SpellStart.
            assert_eq!(holder.event.flags & 0x20, 0);
            assert_eq!(first_action.params.unwrap()[2] & 0x16, 0);
            assert!(units.get(&active_objects, candidate).unwrap().has_ai);
            println!("F6_EVENTAI_SPAWN event=649101 type=11 cast=[10848,9036] target=0 flags=0 branch=normal_cast finished=true");
            // SpellMgr.h:90-108,984-991 skips masked aura checks for targetA1/B0.
            let ai_spell = &live.spell_starts.spells[&3];
            // Spell.cpp:3201 repeats CheckCast in non-strict mode.
            assert_eq!(ai_spell.cooldown_lookups, [2, 1]);
            assert_eq!(
                (ai_spell.stacker_lookups, ai_spell.stacker_attribute_exits),
                (0, 0)
            );
            let t = addon_spells.template(10848).unwrap();
            assert_eq!(ai_spell.check_suffix_lookups, [2, 2, 1]);
            assert_eq!(check_effect_mask(t).unwrap(), 0);
            let range = addon_spells.range_entry(t.rangeindex).unwrap().unwrap();
            println!("F6_COOLDOWN_CONTINUATION range_index={} range_flags={} affected_mask=0 self_targeting=false stacker_lookups=0 location_items_range_power_complete=true", t.rangeindex, range.flags);
            assert_eq!((t.attributes, t.attributesex), (256, 0));
            let old = addon_spells.template(9036).unwrap();
            assert_eq!((old.attributes, old.attributesex), (763363648, 1024));
            assert_eq!(
                (t.effect1, t.effectapplyauraname1, t.effect2, t.effect3),
                (6, 4, 0, 0)
            );
            assert_eq!((t.effectimplicittargeta1, t.effectimplicittargetb1), (1, 0));
            assert_eq!(
                live.auras.first_aura_holder(10848).unwrap().caster,
                candidate
            );
            assert!(live.cooldowns.is_empty());
            assert_eq!(live.spell_starts.spells[&1].creature_cooldown.searches, 0);
            let addon_cooldown = &live.spell_starts.spells[&2].creature_cooldown;
            assert_eq!(addon_cooldown.searches, 1);
            assert_eq!(addon_cooldown.result, Some(CreatureCooldownMatch::Missing));
            assert_eq!(addon_cooldown.fallback, Some(None));
            assert_eq!(live.shapeshift_form, 0);
            assert_eq!(
                active_objects.get(candidate).unwrap().kind,
                SourceObjectKind::Creature
            );
            assert!(!live.totem);
            assert_eq!(live.auras.aura_spell_ids().next(), Some(9036));
            assert_eq!(
                live.auras.first_aura_holder(9036).unwrap().caster,
                candidate
            );
            assert_eq!(ai_spell.power_lookups, [2, 2]);
            assert_eq!(ai_spell.prepare_lookups, [1, 1, 2, 1, 1, 1]);
            assert_eq!(
                (ai_spell.power_cost, ai_spell.cast_time, ai_spell.timer),
                (0, 0, 0)
            );
            assert_eq!(live.cast_speed, 1f32.to_bits());
            // Creature.cpp:338,366; initial-classicmangos.sql:328773,426508.
            assert_eq!(live.loaded_creature.native_display, 5233);
            assert_eq!(
                live.combat_reach,
                Some(creature_models.get(5233).unwrap().words[2])
            );
            assert!(live.target.is_none());
            assert_eq!(
                ais.get(&active_objects, candidate)
                    .unwrap()
                    .base
                    .current_spell,
                Some(4)
            );
            assert_eq!(t.attributesex4 & 0x80, 0);
            assert_eq!(t.attributes & 0x404, 0);
            assert_eq!(t.attributesex & 0x44, 0);
            assert_eq!(old.aurainterruptflags & 4, 0);
            assert_eq!(live.auras.aura_holders().len(), 3);
            println!("F6_PREPARE_AURA_WALK flag=4 ghost_flags={} shroud_flags={} holders_at_prepare=2 state=true force_target=false has_target=false", old.aurainterruptflags, t.aurainterruptflags);
            println!("F6_COOLDOWN_UB_REACH category_null=0 school_shift=0 lockout_shift=0 clock_add=0 form_shift=0");
            assert_eq!(
                (ai_spell.spell, ai_spell.caster, ai_spell.original_caster),
                (10848, candidate, candidate)
            );
            assert_eq!(ai_spell.state, SourceSpellState::Finished);
            assert_eq!(ai_spell.flags, SpellTriggerFlags(0x100));
            assert_eq!(
                (ai_spell.targets.unit, ai_spell.targets.mask),
                (Some(candidate), 2)
            );
            assert_eq!(
                ai_spell.targets.destination,
                Some(
                    active_objects.get(candidate).unwrap().pose.unwrap().0[..3]
                        .try_into()
                        .unwrap()
                )
            );
            assert_eq!(ai_spell.creation_time, 0);
            assert!(ai_spell.filtering_scheme.is_some());
            assert_eq!(
                ai_spell.trigger_data,
                Some(SpellTriggerData {
                    can_trigger: true,
                    negative_effect_mask: 0
                })
            );
            assert!(!ai_spell.referenced && !ai_spell.executing);
            assert_eq!(
                ai_spell.cast_position,
                active_objects.get(candidate).unwrap().pose.unwrap().0
            );
            assert_eq!(live.spell_slots.current(SpellSlot::Generic), Some(4));
            assert_eq!(ai_spell.unique_targets.len(), 1);
            let target = &ai_spell.unique_targets[0];
            assert_eq!(
                (target.unit, target.effect_mask, target.hit_mask),
                (candidate, 1, 1)
            );
            assert!(target.processed && !target.missed);
            assert!(ai_spell.spell_go.is_some());
            assert!(ai_spell.spell_start.is_some());
            // Map.cpp:541-548 and GridNotifiers.cpp:148-155. Both packets visit
            // live world cells, and neither reaches a camera's session read.
            for broadcast in [&ai_spell.start_broadcast, &ai_spell.go_broadcast] {
                assert!(matches!(
                    broadcast,
                    Some(super::super::visibility::SourceBroadcast::Visited { loaded_cells })
                        if *loaded_cells > 0
                ));
            }
            assert_eq!(
                ai_spell.creature_cooldown.result,
                Some(CreatureCooldownMatch::Missing)
            );
            assert_eq!(ai_spell.creature_cooldown.fallback, Some(None));
            assert_eq!(t.stackamount, 0);
            // Unit.cpp:4757 and Spell.cpp:1490,1358. The new ready holder is
            // live; the old one and its unapplied effect await Unit::Update.
            assert!(ai_spell.pending_aura.is_none());
            let new = auras.first_aura_holder(10848).unwrap();
            assert_eq!((new.caster, new.target), (candidate, candidate));
            assert!(new.ready && !new.deleted && new.skip_update);
            let aura = new.effects[0].as_ref().unwrap();
            assert!(aura.applied);
            assert_eq!(aura.kind, 4);
            // seal16-full-01/run/trace.tsv:60478084,60478161.
            assert_eq!((new.apply_time, aura.apply_time), (1794314190, 1794314190));
            let mut deferred = auras.deferred_aura_holders();
            assert_eq!(deferred.len(), 1);
            let (old_id, old) = deferred.next().unwrap();
            assert_eq!(old_id.spell(), 10848);
            assert!(old.deleted && old.effects.iter().all(Option::is_none));
            assert_eq!(old.remove_mode, AuraRemoveMode::Stack as u32);
            assert_eq!(auras.deferred_auras().len(), 1);
            let effect = &auras.deferred_auras()[0];
            assert_eq!(
                (effect.kind, effect.remove_mode, effect.applied),
                (4, 1, false)
            );
            assert_eq!(auras.aura_fields.spells[0], 10848);
            assert_eq!(auras.auras_of_type(4).len(), 1);
            assert_ne!(auras.auras_of_type(4)[0].holder, old_id);
            assert_eq!(auras.auras_of_type(95).len(), 2);
            let latest_ghost = auras
                .aura_holder(auras.auras_of_type(95)[1].holder)
                .unwrap();
            assert!(latest_ghost.ready && latest_ghost.passive);
            let effect = latest_ghost.effects[0].as_ref().unwrap();
            // SpellAuras.cpp:5174,270; trace :60478532 and :60478581.
            assert_eq!(
                (latest_ghost.apply_time, effect.apply_time),
                (1794314190, 1794314190)
            );
            assert!(effect.applied);
            let followup = &live.spell_starts.spells[&4];
            assert_eq!(
                (followup.spell, followup.state),
                (9036, SourceSpellState::Finished)
            );
            assert!(followup.pending_aura.is_none() && !followup.executing && followup.referenced);
            assert_eq!(live.spell_slots.current(SpellSlot::Melee), None);
            assert_eq!(ais.get(&active_objects, candidate).unwrap().base.order, 0);
            let spell = &live.spell_starts.spells[&1];
            assert_eq!(
                (spell.spell, spell.caster, spell.original_caster),
                (9036, candidate, candidate)
            );
            assert_eq!(spell.state, SourceSpellState::Finished);
            assert_eq!(spell.flags, SpellTriggerFlags(1));
            assert_eq!(
                (spell.cast_time, spell.timer, spell.creation_time),
                (0, 0, 0)
            );
            assert_eq!(spell.targets.unit, Some(candidate));
            assert_eq!(spell.targets.mask, 2);
            assert_eq!(
                spell.cast_position,
                active_objects.get(candidate).unwrap().pose.unwrap().0
            );
            assert_eq!(
                spell.trigger_data,
                Some(SpellTriggerData {
                    can_trigger: true,
                    negative_effect_mask: 0
                })
            );
            assert!(
                !spell.referenced && !spell.executing && !spell.client_cast && !spell.item_cast
            );
            assert_guid40520_queue(&live.spell_starts, &live.unit_events);
            assert_eq!(evidence.words[27080], (60477501, 27080, 3988355474));
            println!("F6_ADDON_CAST guid=40520 entry=6491 completed_spells=4 types=[95,4] queued=5 cast_words=0 next_owner={}", refusal.id);
            println!("F6_ADDON_CAST_UB stock_prefix_spells=2 effect_index_out_of_bounds=0 cast_time_signed_add_overflow=0 cast_time_float_to_int=0 school_mask_shift=0 duration_abs=0 max_duration_abs=0 effect_base_sub=0 effect_float_to_int=0 aura_type_enum=0 immunity_effect_index=0");
            println!("F6_SPELL_BROADCAST start_and_go_visited=true camera_session_reads=0 cell_float_to_int_undefined=0");
            assert_eq!(
                zone_lookups[&SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid: 636,
                }],
                [33, 1741]
            );
            assert_eq!(
                evidence.wmo_area_hit.as_deref(),
                Some("3200 42214d13 9 1 568 2860")
            );
            assert!(geometry.wmo_area_lookups.contains(&(
                0,
                [568, 1, 2860],
                Some(geometry.dbc.areas[&1741].flag as u16),
                9,
            )));
            assert!(completed_objects.iter().any(|handle| active_objects
                .get(*handle)
                .unwrap()
                .database_guid
                == 636));
        }
        if slot == 1 {
            let waypoint = active_objects
                .lookup_database(
                    continents.maps[slot].identity,
                    DatabaseLookupKind::Creature,
                    28842,
                )
                .unwrap();
            // Map.cpp:501-504,1335-1348 traversed real loaded cells.
            // GridNotifiers.cpp:32-38 reached no camera, hence no call to
            // Player.cpp:17859 -> Object.cpp:3083 could populate this set.
            assert_eq!(visibility.cameras, 0);
            assert_eq!(visibility.unvisited_guids, 0);
            assert_eq!(visibility.residual_players, 0);
            assert!(active_objects
                .client_guids_i_am_at(waypoint)
                .unwrap()
                .is_empty());
            // Catalog equipment11596/item3368 and pre-start-classicmangos.sql:165687,595844 specify the completed creature's fields.
            let equipment = &equipment_writes[&27923];
            assert_eq!(equipment.id, 11596);
            assert_eq!(
                equipment.slots,
                [
                    SourceVirtualItem {
                        display: 7459,
                        info: [0x0d020e02, 2]
                    },
                    SourceVirtualItem::default(),
                    SourceVirtualItem::default(),
                ]
            );
            assert!(completed_objects.iter().any(|handle| active_objects
                .get(*handle)
                .unwrap()
                .database_guid
                == 27923));
            assert_eq!(refusal.id, active_init::CREATURE_NEXT_OWNER[slot]);
            assert_eq!(
                zone_lookups[&SpawnKey {
                    kind: GridSpawnKind::GameObject,
                    guid: 32019,
                }],
                [405, 2617]
            );
            assert_eq!(
                evidence.wmo_area_hit.as_deref(),
                Some("2471 4275d8f6 10245 0 1771 8359")
            );
            assert!(geometry
                .wmo_area_lookups
                .contains(&(1, [1771, 0, 8359], None, 10245)));
            assert!(completed_objects.iter().any(|handle| active_objects
                .get(*handle)
                .unwrap()
                .database_guid
                == 32019));
            let main = active_objects
                .lookup_database(
                    continents.maps[slot].identity,
                    DatabaseLookupKind::Creature,
                    27010,
                )
                .unwrap();
            // Recording60409483 completes the first main-spell creature.
            assert!(completed_objects.contains(&main));
            let main_live = units.get(&active_objects, main).unwrap();
            assert_eq!(main_live.state & WANDER_ACTIVE, WANDER_ACTIVE);
            assert!(main_live.has_ai && active_objects.get(main).unwrap().in_world);
            let main_ai = &ais.get(&active_objects, main).unwrap().base;
            // Catalog event463303/action1 and spell6660; UnitAI.cpp:966-969.
            assert_eq!(
                (
                    main_ai.main_spell_id,
                    main_ai.main_spell_info,
                    main_ai.main_spell_cost,
                    main_ai.main_attack_mask
                ),
                (6660, Some(6660), 0, 1)
            );
            assert_eq!(main_ai.main_spell_min_range, 5.);
            assert_eq!(event_ais[&main].main_spells, BTreeSet::from([6660]));
            assert_eq!(
                event_ais[&main]
                    .holders
                    .iter()
                    .map(|h| h.event.id)
                    .collect::<Vec<_>>(),
                [463301, 463302, 463303]
            );
            for guid in [27002, 29037] {
                let completed = active_objects
                    .lookup_database(
                        continents.maps[slot].identity,
                        DatabaseLookupKind::Creature,
                        guid,
                    )
                    .unwrap();
                assert!(completed_objects.contains(&completed));
            }
            // Recording60368723 completes GUID28842 after Creature.cpp:882-895.
            assert!(completed_objects.contains(&waypoint));
            let (attached, master, offsets) =
                link_follow.expect("linked MoveFollow reached the base constructor");
            let follower = attached.unit;
            // Recording60533942-60533948 starts Misha's load after Rexxar.
            let misha = active_objects.get(follower).unwrap();
            let misha_fields = units.get(&active_objects, follower).unwrap();
            assert_eq!(
                (misha.database_guid, misha.kind, misha_fields.entry),
                (590005, SourceObjectKind::Creature, 10204)
            );
            assert_eq!(active_objects.get(master).unwrap().database_guid, 29113);
            // Recording60533941 completes Rexxar, then :60534450 completes Misha.
            assert_eq!(completed_objects[197..199], [master, follower]);
            assert!(misha.in_world && misha_fields.has_ai);
            // Respawn poses at trace60531970/60533948 and model rows at
            // pre-start-classicmangos.sql:330733,332387 supply the operands.
            assert_eq!(offsets, [0x403fe66a, 0x409a01b2]);
            let follow = motions[&follower].follow().unwrap();
            // TargetedMovementGenerator.h:34,46-51,184-188 constructs the linked base first.
            assert_eq!(
                units.followers(&active_objects, master).unwrap().ids(),
                [follow.source()]
            );
            assert_eq!(attached, follow.source());
            assert_eq!(
                follow.source(),
                FollowerReferenceId {
                    unit: follower,
                    ordinal: 1
                }
            );
            assert_eq!(follow.target(), Some(master));
            assert_eq!(follow.fields.offsets.map(f32::to_bits), offsets);
            assert!(!follow.fields.main && !follow.fields.possess && !follow.fields.boost);
            assert_eq!(follow.fields.recheck_distance, 0);
            assert_eq!(follow.fields.last_target_position.map(f32::to_bits), [0; 3]);
            assert!(!follow.fields.speed_changed && !follow.fields.target_reached);
            assert!(!follow.fields.face_target && follow.fields.path.is_none());
            assert!(!follow.fields.target_moving && !follow.fields.target_faced);
            assert_eq!(
                units.get(&active_objects, follower).unwrap().state & 0x0018_0000,
                0x0008_0000
            );
            assert!(follow.notices().is_empty());
            println!("F6_LINK_FOLLOW map=1 follower=590005 master=29113 offsets={offsets:x?}");
            let candidate = waypoint;
            let live = units.get(&active_objects, candidate).unwrap();
            assert_eq!(live.state & WANDER_ACTIVE, WANDER_ACTIVE);
            assert!(live.has_ai);
            assert!(active_objects.get(candidate).unwrap().in_world);
            // Unit.cpp:347-351 constructs ALIVE, state=0 and empty slots.
            // This path applied no addon spells, so Spell.cpp:3060-3098 never
            // registered a current spell. IsImmobilizedState, Unit.h:2293,
            // tests ROOT|STUNNED|LOGOUT_TIMER|AI_ROOT, not UNIT_FIELD_FLAGS.
            assert!(active_objects.get(candidate).unwrap().alive);
            assert_eq!(live.state & 0x2000_001a, 0);
            assert!(!live.immobilized());
            assert!(live.spell_starts.spells.is_empty());
            for slot in [
                SpellSlot::Melee,
                SpellSlot::Generic,
                SpellSlot::Channel,
                SpellSlot::AutoRepeat,
            ] {
                assert_eq!(live.spell_slots.current(slot), None);
            }
            assert!(!live
                .spell_slots
                .is_non_melee_spell_casted(&(), &addon_spells, false, false, true, true, false,)
                .unwrap());
            // Unit.cpp:11216 broadcast returns before the completed AI construction.
            // Recording60367004/60368544 proves both BuildIntPath calls before launch.
            assert_eq!(live.state & WANDER_MOVING, WANDER_MOVING);
            // The trace records no spline id here. MoveSplineInit.cpp:27,102
            // establishes id1 for thread7's first launch; :104-117 commits before send.
            assert!(live.movespline.initialized());
            assert_eq!(live.movespline.id(), 1);
            assert_eq!(
                live.movement.as_ref().unwrap().flags.unwrap() & 0x0040_0101,
                0x0040_0101
            );
            assert_eq!(live.spline_outbound.len(), 1);
            assert_eq!(live.spline_outbound[0].unit, candidate);
            assert_eq!(live.spline_outbound[0].transport, None);
            // Recording60433815 consumes equal-bound word12623 at Creature.cpp:1563.
            assert_eq!(random.cursor().raw, 13926); // Trace :60650115 consumes raw 13925.
            assert_eq!(evidence.words[12623], (60433815, 12623, 1799183854));
            relations_evidence::assert_words(&evidence.words);
            assert_eq!(
                &evidence.words[12405..12422],
                &[
                    (60409547, 12405, 3486520897),
                    (60409576, 12406, 2907014274),
                    (60409736, 12407, 4168607143),
                    (60410464, 12408, 2255168339),
                    (60410484, 12409, 3791256556),
                    (60410537, 12410, 619082535),
                    (60410557, 12411, 2227364332),
                    (60410716, 12412, 3477793363),
                    (60410747, 12413, 3624887212),
                    (60410925, 12414, 784206880),
                    (60411662, 12415, 3680951735),
                    (60411683, 12416, 756768607),
                    (60411734, 12417, 2693836351),
                    (60411754, 12418, 3006033073),
                    (60411788, 12419, 3799458557),
                    (60411913, 12420, 1123756501),
                    (60411944, 12421, 392296967),
                ]
            );
            assert_eq!(evidence.words[12422], (60412568, 12422, 3973391667));
            assert_eq!(evidence.words[12112], (60368585, 12112, 257701202));
            assert_eq!(
                lookups
                    .split_inclusive(|lookup| *lookup == "grid_battleground_null")
                    .take(102)
                    .flatten()
                    .filter(|&&s| s == "creature_live_spell_movement_query")
                    .count(),
                1
            );
            // WaypointMovementGenerator.cpp:311 queries once before each initial move.
            // The later paths belong to GUIDs 29103, 50797, and 29113 at recording
            // 60449019, 60506680/60509749, and 60532760/60533794, respectively.
            // Trace :60580464/:60582481/:60583864 adds GUID35138's first move.
            // Trace :60602723 and :60617339 start the two bosses' first moves.
            assert_eq!(
                lookups
                    .iter()
                    .filter(|&&s| s == "creature_live_spell_movement_query")
                    .count(),
                8
            );
            // Recording60409483 completes object73 after Creature.cpp:886-895.
            assert_eq!(completed_objects[72], main);
            let main_calls = lookups
                .split_inclusive(|lookup| *lookup == "grid_battleground_null")
                .nth(72)
                .unwrap();
            assert!(main_calls.ends_with(&[
                "creature_base_ai_constructed",
                "creature_event_ai_constructed",
                "creature_ai_spell_list_changed",
                "creature_ai_just_respawned",
                "creature_instance_respawn_callback",
                "creature_creator_loot_checked",
                "creature_aim_initialize_return",
                "creature_spawn_relay_checked",
                "creature_respawn_group_checked",
                "creature_load_from_db_return",
                "grid_viewpoint_added",
                "grid_battleground_null",
            ]));
            // GUID27036 is the next candidate after trace60433318. Its addon
            // starts at Creature.cpp:932 before the cooldown draw at trace60433815.
            let battle_calls = lookups
                .split_inclusive(|lookup| *lookup == "grid_battleground_null")
                .nth(102)
                .unwrap();
            let addon_applied = battle_calls
                .iter()
                .position(|&lookup| lookup == "creature_addon_applied")
                .unwrap();
            assert!(battle_calls[..addon_applied].ends_with(&[
                "creature_load_addon",
                "creature_db_addon_lookup",
                "creature_template_addon_lookup",
            ]));
            // Recording60413544 completes GUID27677 after its addon18148 self cast.
            let assisted = active_objects
                .lookup_database(
                    continents.maps[slot].identity,
                    DatabaseLookupKind::Creature,
                    27677,
                )
                .unwrap();
            let stopped = units.get(&active_objects, assisted).unwrap();
            let spell = &stopped.spell_starts.spells[&1];
            assert_eq!(
                (spell.spell, spell.caster, spell.targets.unit),
                (18148, assisted, Some(assisted))
            );
            let t = addon_spells.template(18148).unwrap();
            assert_eq!(t.effectimplicittargeta1, 21);
            let filter = SPELL_TARGET_INFO_TABLE[21];
            assert_eq!(
                (filter.target_type, filter.filter, filter.enumerator),
                (
                    SpellTargetImplicitType::Unit,
                    SpellTargetFilter::Helpful,
                    SpellTargetEnumerator::Single
                )
            );
            assert_eq!(spell.original_caster, assisted);
            assert_eq!(stopped.flags & (0x0200_0000 | 8), 0);
            assert!(stopped.has_ai && active_objects.get(assisted).unwrap().in_world);
            assert!(completed_objects.contains(&assisted));
            assert_eq!(spell.state, SourceSpellState::Finished);
            // Recording60433318 completes the later GO32518 while this unit retains its SpellEvent.
            let later = completed_objects
                .iter()
                .position(|&object| {
                    active_objects.get(object).unwrap().kind == SourceObjectKind::GameObject
                        && active_objects.get(object).unwrap().database_guid == 32518
                })
                .unwrap();
            assert!(
                completed_objects
                    .iter()
                    .position(|&object| object == assisted)
                    .unwrap()
                    < later
            );
            assert!(stopped
                .unit_events
                .inventory()
                .queued
                .iter()
                .any(|event| event.event_id == spell.event.unwrap()
                    && event.kind == SourceUnitEvent::Spell { spell: spell.id }));
            let aura = stopped.auras.first_aura_holder(18148).unwrap();
            assert!(aura.ready && aura.permanent);
            let effect = aura.effects[0].as_ref().unwrap();
            assert!(effect.applied && effect.periodic);
            assert_eq!(
                (effect.kind, effect.periodic_time, effect.periodic_timer),
                (23, 3000, 3000)
            );
            // Catalog spell7165/ordinal5721 and legacy list463600/ordinal987 precede trace60433815.
            let battle_caster = active_objects
                .lookup_database(
                    continents.maps[slot].identity,
                    DatabaseLookupKind::Creature,
                    27036,
                )
                .unwrap();
            let current = units.get(&active_objects, battle_caster).unwrap();
            let cast = &current.spell_starts.spells[&1];
            let battle = addon_spells.template(7165).unwrap();
            assert_eq!(
                (
                    cast.spell,
                    battle.attributes,
                    battle.category,
                    battle.recoverytime,
                    battle.categoryrecoverytime
                ),
                (7165, 327696, 47, 0, 180000)
            );
            assert_eq!(cast.state, SourceSpellState::Finished);
            assert!(current.has_ai && active_objects.get(battle_caster).unwrap().in_world);
            assert!(completed_objects.contains(&battle_caster));
            assert_eq!(
                active_objects.get(battle_caster).unwrap().kind,
                SourceObjectKind::Creature
            );
            assert!(!current.totem);
            let list = current.spell_list.as_ref().unwrap();
            assert_eq!(
                (
                    list.spells[&0].spell_id,
                    list.spells[&0].repeat,
                    list.spells[&0].flags
                ),
                (7165, [0, 0], 0)
            );
            assert_eq!(cast.creature_cooldown.searches, 1);
            assert_eq!(
                cast.creature_cooldown.result,
                Some(CreatureCooldownMatch::Spell(0))
            );
            assert_eq!(cast.creature_cooldown.fallback, None);
            assert_eq!(
                cast.creature_cooldown.master,
                Some(SourceMasterLookup {
                    charmer: SourceControlLookup::EmptyGuid,
                    creator: Some(SourceControlLookup::EmptyGuid),
                })
            );
            let cooldown = &current.cooldowns.spells[&7165];
            assert_eq!(
                (
                    cooldown.spell_expiry,
                    cooldown.category_expiry,
                    cooldown.permanent
                ),
                (0, 180000, false)
            );
            assert_eq!(current.cooldowns.categories[&47], 7165);
            assert_eq!(current.shapeshift_form, 17);
            assert_eq!(current.threat_modifier, [0.8; 7]);
            assert_eq!(
                current.auras.aura_spell_ids().collect::<Vec<_>>(),
                [7165, 21156]
            );
            for spell in [7165, 21156] {
                let aura = current.auras.first_aura_holder(spell).unwrap();
                assert!(aura.ready && aura.permanent);
                assert!(aura.effects[0].as_ref().unwrap().applied);
                assert_eq!(aura.apply_time, 1794314190);
            }
            assert_eq!(
                current.auras.first_aura_holder(21156).unwrap().triggered_by,
                Some(7165)
            );
            let events = current.unit_events.inventory();
            assert!(events.queued.iter().any(|event| {
                event.event_id == cast.event.unwrap()
                    && event.kind == SourceUnitEvent::Spell { spell: cast.id }
            }));
            // Trace lines 60433815 and 60434041 anchor the stance and its aura clocks.
            // Spell.cpp:3008-3009 queues the outer event before SpellAuras.cpp:4291-4295.
            let boost = &current.spell_starts.spells[&2];
            assert_eq!((cast.id, boost.id, boost.spell), (1, 2, 21156));
            assert_eq!(boost.state, SourceSpellState::Finished);
            assert_eq!(boost.triggered_by_aura, Some(7165));
            let spell_events: Vec<_> = events
                .queued
                .iter()
                .filter_map(|event| match event.kind {
                    SourceUnitEvent::Spell { spell } if spell == cast.id || spell == boost.id => {
                        Some((spell, event.event_id))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                spell_events,
                [
                    (cast.id, cast.event.unwrap()),
                    (boost.id, boost.event.unwrap())
                ]
            );
            println!("F6_CREATURE_COOLDOWN map=1 guid=27036 spell=7165 repeat=[0,0] flags=0 raw_line=60433815 raw_index=12623 raw_value=1799183854 result=0 category_expiry=180000 master=empty_guids");
            // Recording60365908 returns no group; MotionMaster.cpp:87-94
            // retains path zero/PATH_NO_PATH and enters InitializeWaypointPath.
            assert!(groups
                .creature_group(continents.maps[slot].identity, 28842)
                .unwrap()
                .is_none());
            assert!(lookups.windows(4).any(|calls| calls
                == [
                    "creature_waypoint_default_path",
                    "creature_loaded_waypoint_manager",
                    "creature_live_spell_movement_query",
                    "creature_motion_initialized",
                ]));
            // A path and its initialized first cursor are required to reach
            // SendNextWayPointPath's spell query, cpp:94-131,305-311.
            let (origin, path) = waypoint_paths
                .select(4726, 28842, 0, WaypointOrigin::None)
                .unwrap();
            assert_eq!(origin, WaypointOrigin::Guid);
            let path = path.unwrap();
            // pre-start-classicmangos.sql:355066-355088 fixes all 23 nodes.
            assert_eq!(path.len(), 23);
            assert_eq!(path.first_key_value().unwrap().0, &1);
            assert_eq!(
                path[&1],
                SourceWaypointNode {
                    position: [-1318.92, 1068.75, 92.4753],
                    orientation: Some(0.),
                    delay: 0,
                    script: 0,
                }
            );
            // Trace60366140 is word12111; the next word is AI's :60368585.
            // Loading and initializing this path introduces no draw or SQL.
            assert_eq!(
                groups
                    .waypoint_formation_path(continents.maps[slot].identity, 28842)
                    .unwrap(),
                None
            );
            let data = owner
                .spawns
                .complete_data(SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid: 28842,
                })
                .unwrap()
                .unwrap();
            // Recording60365891 fixes the waypoint type before MotionMaster.cpp:94.
            assert_eq!(data.row.movement, 2);
            // Snapshot106020 and recording60349485 fix this IsSwimmable pose; recording60349570 returns no VMap liquid.
            let snake = owner
                .spawns
                .complete_data(SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid: 28501,
                })
                .unwrap()
                .unwrap();
            assert_eq!(&snake.row.pose[..3], &[-1409.87, 1547.21, 60.7043]);
            let cell = benilla_native_terrain::grid_height::coordinates(
                snake.row.pose[0],
                snake.row.pose[1],
            )
            .unwrap()
            .1;
            assert_eq!(cell, [82, 12]);
            let frozen = recording
                .ancestors()
                .find(|path| path.ends_with("benilla-config/full-server/f1"))
                .unwrap();
            let bytes = std::fs::read(frozen.join("products/maps/0013429.map")).unwrap();
            assert_eq!(
                Sha256Id::of(&bytes),
                Sha256Id::parse("f0a132e3818e263a2b8faa6e0941f214123facef75e5ee4be9c696b1bb4deb7a")
                    .unwrap()
            );
            let liquid_offset = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
            assert_eq!(liquid_offset, 66626);
            assert_eq!(&bytes[liquid_offset..liquid_offset + 4], b"MLIQ");
            assert_eq!(
                &bytes[liquid_offset + 8..liquid_offset + 12],
                &[7, 109, 14, 14]
            );
            let liquid = &geometry.terrain(1).unwrap().grid(34, 29).unwrap().liquid;
            assert_eq!((liquid.offset, liquid.size), ([7, 109], [14, 14]));
            // GridMap.cpp:598-600 returns NO_WATER because 82 - 109 is negative.
            assert!(cell[0] < usize::from(liquid.offset[1]));
        }
        let completed_marks = lookups
            .iter()
            .filter(|lookup| **lookup == "grid_battleground_null")
            .count();
        let motion_initializations = lookups
            .iter()
            .filter(|lookup| **lookup == "creature_motion_initialized")
            .count() as u64;
        if let Some(first) = first_creature {
            attempt = first.attempt;
            candidate = first.candidate;
            entry = first.entry;
            lookups = first.lookups;
            init_entry_fields = first.init_entry_fields;
            bound_guid = active_objects.get(candidate).unwrap().guid;
            creature_display_chance = first.display_chance;
            creature_model = first.model;
            creature_entry = Some(first.fields);
            visibility = first.visibility;
            creature_battleground = first.battleground;
            creature_zone = first.zone;
            creature_linking_trigger = first.linking_trigger;
            selected_ai = first.selected_ai;
            go_load_operands = first.go_load_operands;
            go_factory_elevator = first.go_factory_elevator;
            go_animprogress = first.go_animprogress;
            go_battleground = first.go_battleground;
            go_zone = first.go_zone;
            go_script_id = first.go_script_id;
        }
        let fields = creature_entry.as_ref().unwrap();
        println!(
            "F6_ADDON_APPLIED map={slot} guid={} entry={entry} selected={:?} fields={:?} radius={:?} respawn_delay={:?} corpse_delay={} respawn_time={:?} clock={:?} notify_delay={:?}",
            attempt.guid,
            creature_addons.get(HighGuid::Creature, attempt.guid, entry),
            fields.addon,
            fields.respawn_radius,
            fields.respawn_delay,
            fields.corpse_delay,
            fields.respawn_time,
            fields.respawn_clock,
            fields.notify_delay
        );
        assert_eq!(fields.respawn_time, Some(0));
        assert_eq!(fields.notify_delay, Some(5000));
        let selected_ai = selected_ai.unwrap();
        let template = creature_templates
            .get(if slot == 0 { 687 } else { 4692 })
            .unwrap();
        // catalog/loaded_templates.rs:5 fixes ExtraFlags at column 23; :47-49
        // copies that integer cell to words[23]. Creature.h:640-641 reads these bits.
        let extra_flags = template.words[23];
        let ai_name = std::str::from_utf8(&template.strings[2]).unwrap();
        let guard = extra_flags & 0x400 != 0;
        let no_aggro = extra_flags & 2 != 0;
        assert_eq!(fields.faction, template.words[13]);
        // Unit.cpp:6228-6238 and DBCStructure.h:441-446 use the retained DBCs.
        let neutral = ai_factions.neutral_to_all(template.words[13]);
        assert!(!guard && !no_aggro && !neutral);
        assert_eq!(selected_ai.inputs().effective_entry, template.words[0]);
        assert_eq!(selected_ai.inputs().ai_name, ai_name);
        assert_eq!(selected_ai.inputs().guard, guard);
        assert_eq!(selected_ai.inputs().no_aggro_on_sight, no_aggro);
        assert_eq!(selected_ai.inputs().neutral_to_all, neutral);
        // CreatureAISelector.cpp:63-83; CreatureEventAI.cpp:51-60:
        // empty-name proactive EventAI scores 200, above NullAI's 1; other
        // factories reject this ordinary non-pet/non-totem/non-guard creature.
        let (expected_branch, expected_key) = if ai_name.is_empty() {
            assert_eq!(slot, 0);
            (AiSelectionBranch::Permit, "EventAI")
        } else {
            assert_eq!(slot, 1);
            (AiSelectionBranch::ExplicitName, ai_name)
        };
        assert_eq!(selected_ai.branch(), expected_branch);
        let AiExecutable::Builtin(binding) = selected_ai.executable() else {
            panic!("the loaded creature must select a registered builtin factory");
        };
        assert_eq!(binding.key, expected_key);
        // CreatureAIRegistry.cpp:41 registers EventAI to CreatureEventAI.
        assert_eq!(binding.kind, BuiltinCreatureAi::Event);
        let live = units.get(&active_objects, candidate).unwrap();
        assert!(live.has_ai);
        assert_eq!(live.state, WANDER_ACTIVE);
        let base = &ais.get(&active_objects, candidate).unwrap().base;
        assert_eq!(base.unit, candidate);
        assert_eq!(base.action_count(), 0);
        assert_eq!(base.react_state, 2);
        assert!(base.melee_enabled && base.dismount_on_aggro);
        let event_ai = &event_ais[&candidate];
        if slot == 0 {
            assert!(event_ai.holders.is_empty());
            assert_eq!(event_ai.phase, 0);
            assert_eq!(event_ai.depth, 0);
            assert_eq!(event_ai.clock.remaining, 500);
            assert_eq!(live.spell_list_disabled, None);
            assert!(live.spell_list.is_none());
            assert_eq!(
                (
                    base.main_spell_id,
                    base.main_spell_cost,
                    base.main_attack_mask
                ),
                (0, 0, 0)
            );
            assert_eq!(base.main_spell_min_range, 0.);
            assert_eq!(base.main_spell_info, None);
            assert!(event_ai.main_spells.is_empty());
        }
        if slot == 1 {
            assert_eq!(event_ai.clock.remaining, 500);
            assert_eq!(event_ai.clock.debt, 0);
            assert_eq!(event_ai.holders.len(), 1);
            assert_eq!(event_ai.holders[0].event.id, 469201);
            assert_eq!(event_ai.holders[0].event.kind, 0);
            assert!(event_ai.holders[0].enabled);
            assert_eq!(event_ai.holders[0].timer, 5376);
            assert_eq!(base.main_spell_id, 0);
        }
        println!(
            "F6_AI_CONSTRUCTED map={slot} main_spell={} holders={} spell_list_disabled={:?} spells={:?} in_combat={}",
            base.main_spell_id,
            event_ai.holders.len(),
            live.spell_list_disabled,
            live
                .spell_list
                .as_ref()
                .map(|list| list.spells.len()),
            live.in_combat()
        );
        println!(
            "F6_AI_LIFECYCLE map={slot} clock={:?} holders={:?}",
            event_ai.clock,
            event_ai
                .holders
                .iter()
                .map(|holder| (
                    holder.event.id,
                    holder.event.kind,
                    holder.timer,
                    holder.enabled
                ))
                .collect::<Vec<_>>()
        );
        // Unit.cpp:10962, EventProcessor.cpp:23,103-107,124-126: no host clock read.
        assert_eq!(live.unit_events.clock_millis(), 0);
        assert!(live.unit_events.is_ai_notify_scheduled());
        let queued = live.unit_events.inventory().queued;
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].kind, SourceUnitEvent::AiNotify);
        assert_eq!((queued[0].add_time_ms, queued[0].exec_time_ms), (0, 5000));
        assert!(!queued[0].to_abort);
        assert_eq!(fields.motion_state, Some(WANDER_ACTIVE));
        assert!(!fields.can_aggro);
        let idle_count = completed_objects
            .iter()
            .filter(|handle| {
                let object = active_objects.get(**handle).unwrap();
                object.kind == SourceObjectKind::Creature
                    && owner
                        .spawns
                        .complete_data(SpawnKey {
                            kind: GridSpawnKind::Creature,
                            guid: object.database_guid,
                        })
                        .unwrap()
                        .unwrap()
                        .row
                        .movement
                        == 0
            })
            .count() as u64;
        // MotionMaster.cpp:79-81. The seven added Idle loaders record movement=0
        // at 60433354, 60446082, 60475692, 60476575, 60482101, 60521606, 60522958.
        // Misha and GUID50950 also select Idle, at recording60533948 and 60535163.
        // GUID35888, GUID35890 and GUID35884 add Idle at 60551017, 60551773 and 60552428.
        // Map 1 adds Idle at :60553183, :60555425, :60556244, :60556983,
        // :60557655, :60558325 and :60559033.
        // Map 0 adds Idle at :60476980, :60497080, :60507541, :60508292,
        // :60509039, :60510051, :60510915 and :60516693.
        // GUID5015 enters Idle (trace :60575680, initial-classicmangos.sql:85040).
        // GUIDs 4246, 4085, 4178, 4177 and 4206 add Idle: :60580674, :60596324,
        // :60604394, :60605820, :60623342; stock SQL :84305, :84147, :84238, :84237, :84265.
        // Ten more completed movement-type-zero creatures, :60639124-60700643.
        // GUID4191, GUID4193, and GUID4208 add Idle at :60748051, :60749159, and :60757810.
        // Fifteen more Idle creatures complete at :60774468-60979028.
        assert_eq!(idle_count, if slot == 0 { 52 } else { 23 });
        assert_eq!(
            geometry.pathfinder_constructor_completions,
            // WaypointMovementGenerator.cpp:264 constructs per BuildIntPath.
            // Map 0 records 46 paths for 28 waypoint creatures.
            // Map 1 calculates sixteen paths for eight waypoints (:60653716 last).
            // Static Idle constructs none (MotionMaster.cpp:79-81).
            pathfinders_before + motion_initializations - idle_count
                + if slot == 0 {
                    // The stopped GUID12058 initialized Idle but did not complete.
                    46 - 28 - 1
                } else {
                    16 - 8
                }
        );
        assert_eq!(
            active_objects.viewpoint_cell(candidate).unwrap(),
            continents.maps[slot].cells.registered_cell(candidate)
        );
        assert!(!active_objects.get(candidate).unwrap().is_new_object);
        assert_eq!(
            fields.respawn_clock,
            Some((
                if slot == 0 { 60383356 } else { 60310427 },
                if slot == 0 { 60383360 } else { 60310429 },
                1794314190
            ))
        );
        assert!(active_objects.get(candidate).unwrap().in_world);
        assert!(continents.maps[slot]
            .cells
            .registered_cell(candidate)
            .is_some());
        assert_eq!(
            active_objects.lookup_database(
                continents.maps[slot].identity,
                DatabaseLookupKind::Creature,
                attempt.guid
            ),
            Some(candidate)
        );
        assert_eq!(completed_objects.len(), completed);
        assert_eq!(completed_marks, completed);
        if slot == 0 {
            assert_eq!(
                (attempt.guid, entry, attempt.kind),
                (1996, 687, GridSpawnKind::Creature)
            );
            assert_eq!((attempt.cell.x(), attempt.cell.y()), (112, 525));
            assert!(lookups.ends_with(&[
                "creature_addon_applied",
                "creature_create_return",
                "creature_respawn_coordinate_radius",
                "creature_respawn_delay_draw",
                "creature_persistent_respawn_read",
                "creature_respawn_calendar_consumed",
                "creature_spawned_by_linked_mob_lookup",
                "creature_map_add_grid_link",
                "creature_world_guid_registered",
                "creature_base_add_to_world",
                "creature_schedule_ai_notify",
                "creature_ai_notify_enqueued",
                "creature_force_active_checked",
                "creature_spawn_count_checked",
                "creature_map_active_checked",
                "creature_map_viewpoint_added",
                "creature_map_visibility_complete",
                "creature_motion_initialized",
                "creature_ai_script_registry_lookup",
                "creature_ai_script_null",
                "creature_ai_registry_bound",
                "creature_ai_selected",
                "creature_base_ai_constructed",
                "creature_event_ai_constructed",
                "creature_ai_spell_list_changed",
                "creature_ai_just_respawned",
                "creature_instance_respawn_callback",
                "creature_creator_loot_checked",
                "creature_aim_initialize_return",
                "creature_spawn_relay_checked",
                "creature_respawn_group_checked",
                "creature_load_from_db_return",
                "grid_viewpoint_added",
                "grid_battleground_null",
            ]));
            assert_eq!(
                bound_guid,
                Some(SourceGuid::with_entry(HighGuid::Creature, 687, 1996))
            );
            assert_eq!(
                completed_objects
                    .iter()
                    .map(|handle| active_objects.get(*handle).unwrap().database_guid)
                    .collect::<Vec<_>>(),
                // The old prefix ends at :60476901. The continuation is
                // pinned by the constructor_loader_success receipt.
                [
                    18, 22, 67, 8999, 1996, 2402, 1992, 1993, 2348, 1987, 636, 1989, 32843, 1240,
                    1979, 1907, 1908, 1342, 1991, 1917, 1918, 1977, 1970, 1909, 1910, 1965, 1967,
                    1990, 1983, 1335, 77576, 1904, 1905, 1985, 1975, 947, 1902, 7876, 1911, 1959,
                    953, 8108, 191715, 4418, 1944, 1955, 758, 951, 1871, 2482, 922, 1867, 8043,
                    920, 32086, 2259, 2500, 2490, 2123, 1713, 2253, 2270, 7337, 2391, 7514, 1978,
                    1972, 1695, 2383, 2240, 2374, 1858, 1969, 1639, 2231, 2236, 1466, 1682, 35,
                    529, 530, 531, 532, 533, 2247, 77294, 1679,
                ]
                .into_iter()
                .chain(
                    evidence
                        .continuation_completed
                        .iter()
                        .map(|(_, guid)| *guid)
                )
                .collect::<Vec<_>>()
            );
            for (index, handle) in completed_objects.iter().enumerate() {
                let object = active_objects.get(*handle).unwrap();
                assert!(object.in_world);
                let grid = continents.maps[slot]
                    .cells
                    .registered_cell(*handle)
                    .unwrap()
                    .grid;
                if index < 10 {
                    assert_eq!(grid, attempt.grid);
                }
                assert_eq!(grid.map, continents.maps[slot].identity);
                if object.kind == SourceObjectKind::GameObject {
                    assert_eq!(
                        gameobjects.get(*handle).unwrap().ai.is_some(),
                        object.database_guid == 15508
                    );
                } else {
                    assert!(units.get(&active_objects, *handle).unwrap().has_ai);
                }
            }
        } else {
            assert_eq!(
                completed_objects
                    .iter()
                    .map(|handle| active_objects.get(*handle).unwrap().database_guid)
                    .collect::<Vec<_>>(),
                [
                    28044, 28249, 87282, 27655, 28005, 28828, 29163, 28064, 29147, 6253, 28201,
                    27984, 28210, 30207, 28239, 28020, 28353, 28008, 27280,
                    // Recording60348453,60349187,60350793,60352132 precedes GO32019 at :60353621.
                    27923, 28501, 28014, 28837,
                    // Recording60354549-60365868, constructor_loader_success.
                    32019, 27279, 27281, 28857, 28819, 28007, 28043, 28863, 28101, 29038,
                    // Recording60368723-60401103, constructor_loader_success.
                    28842, 28227, 28023, 28045, 29156, 28117, 28910, 28015, 28801, 28858, 28217,
                    28111, 28021, 28834, 28224, 28120, 6211, 28116, 29043, 27990, 27999, 28847,
                    28827, 27982, 28093, 28219, 28245, 28057, 28062, 29192, 27000, 28844, 28078,
                    28900, 29036, 28883, 27001, 28107, 28110,
                    // Recording60409483,60410616,60411828 completes these three creatures.
                    27010, 27002, 29037,
                ]
                .into_iter()
                .chain(relations_evidence::COMPLETED.iter().map(|(_, guid)| *guid))
                .chain(
                    evidence
                        .continuation_completed
                        .iter()
                        .map(|(_, guid)| *guid)
                )
                .collect::<Vec<_>>()
            );
        }
        let (
            attempt,
            candidate,
            entry,
            lookups,
            go_load_operands,
            go_factory_elevator,
            go_animprogress,
            go_battleground,
            go_zone,
            go_script_id,
            visibility,
            init_entry_fields,
            bound_guid,
            creature_model,
            creature_entry,
            creature_display_chance,
        ) = if let Some(first) = first_go.filter(|_| slot == 0) {
            let bound = active_objects.get(first.candidate).unwrap().guid;
            (
                first.attempt,
                first.candidate,
                first.entry,
                first.lookups,
                first.load_operands,
                first.factory_elevator,
                first.animprogress,
                first.battleground,
                first.zone,
                first.script_id,
                first.visibility,
                None,
                bound,
                None,
                None,
                None,
            )
        } else {
            (
                attempt,
                candidate,
                entry,
                lookups,
                go_load_operands,
                go_factory_elevator,
                go_animprogress,
                go_battleground,
                go_zone,
                go_script_id,
                visibility,
                init_entry_fields,
                bound_guid,
                creature_model,
                creature_entry,
                creature_display_chance,
            )
        };
        println!(
            "F6_CONSTRUCTED_COLLISION_HEIGHT map={slot} reached_441={} swimming_set={} swimming_not_set={} display_assert_11952={} model_assert_11954={} scope=recorded_startup_prefix_only",
            swimming_census.reached_441,
            swimming_census.swimming_set,
            swimming_census.swimming_not_set,
            swimming_census.display_assert_11952,
            swimming_census.model_assert_11954
        );
        assert_eq!(
            lookups,
            if slot == 0 {
                vec![
                    "go_template",
                    "go_event_guid",
                    "go_spawn_group",
                    "go_random_entries",
                    "go_force_dynamic",
                    "go_template",
                    "go_template",
                    "go_model",
                    "go_battleground",
                    "go_zone",
                    "go_outdoor_pvp_get_script",
                    "go_instance_data",
                    "go_instance_on_object_create",
                    "go_large_object_predicate",
                    "go_persistent_respawn_read",
                    "go_model", // GameObject.cpp:126 inserts the model during AddToWorld.
                    "go_model", // GameObject.cpp:131 enables collision after world membership.
                    "go_map_viewpoint_added",
                    "go_map_visibility_traversal",
                    "go_map_visibility_complete",
                    "go_ai_script_factory_lookup",
                    "go_ai_gameobject_info",
                    "go_ai_script_registry_lookup",
                    "go_ai_reset_null",
                    "go_ai_just_spawned_skipped",
                    "go_instance_data",
                    "go_instance_on_object_spawn",
                ]
            } else {
                vec![
                    "creature_data",
                    "existing_creature",
                    "spawn_group",
                    "conditional_skipped_nonzero_entry",
                    "random_skipped_nonzero_entry",
                    "active_event",
                    "creature_template",
                    "creature_guid",
                    "creature_create_from_proto",
                    "init_entry_template",
                    "init_entry_fields",
                    "creature_spawn_template_model",
                    "creature_display_chance",
                    "creature_display_draw",
                    "creature_model_random_gender",
                    "creature_native_display_speed_gender_power",
                    "creature_equipment_name_cast_levitate",
                    "creature_init_entry_remainder",
                    "creature_level_stats_spawn",
                    "creature_update_entry_flags_stats_pvp",
                    "creature_map_spell_list_lookup",
                    "creature_spell_list_copy_availability",
                    "creature_immunity_set_lookup",
                    "creature_immunity_set_updated",
                    "creature_critter_chain_immunity",
                    "creature_event_and_string_gates",
                    "creature_static_flags_reset",
                    "creature_select_final_point_static",
                    "creature_final_position_relocated_valid",
                    "creature_create_battleground",
                    "creature_create_zone",
                    "creature_outdoor_pvp_get_script",
                    "creature_instance_data",
                    "creature_instance_on_creature_create",
                    "creature_linking_slave_lookup",
                    "creature_linking_trigger_lookup",
                    "creature_load_addon",
                    "creature_db_addon_lookup",
                    "creature_template_addon_lookup",
                    "creature_addon_applied",
                    "creature_create_return",
                    "creature_respawn_coordinate_radius",
                    "creature_respawn_delay_draw",
                    "creature_persistent_respawn_read",
                    "creature_respawn_calendar_consumed",
                    "creature_spawned_by_linked_mob_lookup",
                    "creature_map_add_grid_link",
                    "creature_world_guid_registered",
                    "creature_base_add_to_world",
                    "creature_schedule_ai_notify",
                    "creature_ai_notify_enqueued",
                    "creature_force_active_checked",
                    "creature_spawn_count_checked",
                    "creature_map_active_checked",
                    "creature_map_viewpoint_added",
                    "creature_map_visibility_complete",
                    "creature_motion_initialized",
                    "creature_ai_script_registry_lookup",
                    "creature_ai_script_null",
                    "creature_ai_registry_bound",
                    "creature_ai_selected",
                    "creature_base_ai_constructed",
                    "creature_event_ai_constructed",
                    "creature_ai_spell_list_changed",
                    "creature_ai_just_respawned",
                    "creature_instance_respawn_callback",
                    "creature_creator_loot_checked",
                    "creature_aim_initialize_return",
                    "creature_spawn_relay_checked",
                    "creature_respawn_group_checked",
                    "creature_load_from_db_return",
                    "grid_viewpoint_added",
                    "grid_battleground_null",
                ]
            }
        );
        assert_eq!(refusal.id, active_init::CREATURE_NEXT_OWNER[slot]);
        assert_eq!(
            (attempt.grid.x, attempt.grid.y),
            if slot == 0 { (7, 32) } else { (29, 34) }
        );
        assert_eq!(
            evidence.first_grid,
            Some((
                if slot == 0 { 60382091 } else { 60310123 },
                attempt.grid.x,
                attempt.grid.y
            ))
        );
        assert_eq!(attempt.guid, if slot == 0 { 18 } else { 28044 });
        // Last allocated objects are at :60982852 (map 0) and :60649360 (map 1).
        assert_eq!(candidates, if slot == 0 { 451 } else { 291 });
        assert_eq!(entry, if slot == 0 { 3211 } else { 4692 });
        if slot == 0 {
            assert_eq!(init_entry_fields, None);
            assert_eq!(
                bound_guid,
                Some(SourceGuid::with_entry(HighGuid::GameObject, 3211, 18))
            );
            assert!(creature_model.is_none() && creature_entry.is_none());
            let object = gameobjects.get(candidate).unwrap();
            assert_eq!(
                (
                    object.display,
                    object.template.kind,
                    object.template.scale_bits
                ),
                (22, 5, 1f32.to_bits())
            );
            assert_eq!(
                (object.state, object.animation, object.dynamic_flags),
                (1, 100, 9)
            );
            assert!(object.has_model);
            assert!(object.ai.is_none());
            assert_eq!(object.respawn_delay, 900);
            assert_eq!(object.respawn_time, 0);
            assert!(object.spawned_by_default && object.spawn.is_some());
            assert_eq!(
                states
                    .state(continents.maps[slot].persistent.unwrap())
                    .unwrap()
                    .respawn(GridSpawnKind::GameObject, attempt.guid),
                0
            );
            assert!(active_objects.get(candidate).unwrap().in_world);
            assert!(!active_objects.get(candidate).unwrap().is_new_object);
            assert_eq!(
                active_objects.viewpoint_cell(candidate).unwrap(),
                continents.maps[slot].cells.registered_cell(candidate)
            );
            assert!(visibility.considered_cells > 0 && visibility.loaded_cells > 0);
            assert_eq!(
                (
                    visibility.cameras,
                    visibility.unvisited_guids,
                    visibility.residual_players
                ),
                (0, 0, 0)
            );
            assert_eq!(
                go_script_id,
                Some(gameobject_templates.get(entry).unwrap().words[37])
            );
            assert_eq!(go_script_id, Some(0));
            assert_eq!(
                evidence.first_candidate_rng,
                [("urand".into(), 601, 900, 900, Some(900))]
            );
            assert_eq!(go_battleground, Some(false));
            assert_eq!(
                go_battleground,
                Some(continents.maps[slot].entry.battleground())
            );
            assert_eq!(go_zone, Some(33));
            let data = owner
                .spawns
                .complete_data(SpawnKey {
                    kind: GridSpawnKind::GameObject,
                    guid: attempt.guid,
                })
                .unwrap()
                .unwrap();
            assert_eq!(
                go_load_operands,
                Some((
                    entry,
                    data.row.pose.map(f32::to_bits),
                    data.row.rotation.map(f32::to_bits)
                ))
            );
            assert_eq!(
                go_factory_elevator,
                Some(gameobject_templates.get(entry).unwrap().words[1] == 11)
            );
            assert_eq!(go_animprogress, Some(data.animprogress));
            assert_eq!(go_animprogress, Some(100));
            assert_eq!(creature_display_chance, None);
            assert_eq!(object.position_bits, data.row.pose.map(f32::to_bits));
            assert_eq!(object.rotation_bits, data.row.rotation.map(f32::to_bits));
            assert_eq!(
                active_objects.get(candidate).unwrap().map,
                Some(continents.maps[slot].identity)
            );
        } else {
            assert_eq!(go_load_operands, None);
            assert_eq!(go_factory_elevator, None);
            assert_eq!(go_animprogress, None);
            assert_eq!((go_battleground, go_zone), (None, None));
            let data = owner
                .spawns
                .complete_data(SpawnKey {
                    kind: GridSpawnKind::Creature,
                    guid: attempt.guid,
                })
                .unwrap()
                .unwrap();
            assert_eq!(data.spawn_template, Some(0));
            assert_eq!(spawn_templates.model_id(data).unwrap(), 0);
            assert_eq!(creature_display_chance, Some(100));
            let template = creature_templates.get(entry).unwrap();
            assert_eq!(entry, 4692);
            assert_eq!((template.words[17], template.words[23]), (1, 0));
            assert_eq!(template.words[17] & 2, 0, "Creature.cpp:439 short-circuits");
            // Creature.cpp:439-444 adds 34 probes after GUID27036. The template
            // operands are in snapshot rows 423578,425240,425616,425627,427039,
            // 427112,428051,428062. Trace60438284 through Misha's load records
            // no VMap liquid at any of those probes; the bound terrain is dry.
            assert_eq!(
                swimming_census,
                SourceCreatureSwimmingCensus {
                    // Creature.cpp:441 adds eight probes at recording60540431, 60545949,
                    // 60547348, 60548644, 60551058, 60551791, 60552460 and 60553213.
                    // The next 26 reads run from trace :60554170 through :60601839.
                    // Six further probes run from :60608903 through :60616191.
                    // Eleven more dry probes (:60623557 through :60649738).
                    reached_441: 88,
                    swimming_set: 4,
                    swimming_not_set: 84,
                    ..SourceCreatureSwimmingCensus::default()
                }
            );
            println!(
                "F6_CONSTRUCTED_COLLISION_SHORT_CIRCUIT entry={entry} template_ordinal={} inhabit_type={} extra_flags={} source_line=439 GetCollisionHeight_calls=0 IsSwimmable_calls=0",
                template.ordinal, template.words[17], template.words[23]
            );
            assert_eq!(
                init_entry_fields,
                Some((entry, template.words[14], [0, template.words[36] as u8]))
            );
            assert_eq!(
                creature_model.unwrap(),
                creature_models.get(1192).unwrap().words
            );
            let fields = creature_entry.as_ref().unwrap();
            assert_eq!(
                (fields.native_display, fields.display, fields.power),
                (1192, 1192, 1)
            );
            assert_eq!(u32::from(fields.gender), creature_model.unwrap()[5]);
            assert_eq!(
                (fields.radius, fields.reach),
                (creature_model.unwrap()[1], creature_model.unwrap()[2])
            );
            assert_eq!(fields.base_speeds, [1f32.to_bits(), 1.14286f32.to_bits()]);
            assert_eq!(fields.speed_rates, fields.base_speeds);
            assert_eq!(fields.model_run_speed, creature_model.unwrap()[4]);
            assert_eq!((fields.equipment, fields.virtual_items), (0, [0; 3]));
            assert_eq!(fields.name, b"Dread Swoop");
            assert_eq!(fields.cast_speed, 1f32.to_bits());
            assert_eq!(
                (
                    fields.movement_flags,
                    fields.default_movement,
                    fields.damage_school
                ),
                (0x100, 1, 0)
            );
            assert_eq!(fields.capabilities, [true, true, false, false, true, false]);
            assert_eq!(fields.detection, 18f32.to_bits());
            assert_eq!(
                (fields.corpse_delay, fields.interaction_pause),
                (270, 180000)
            );
            assert_eq!((fields.sheath_state, fields.misc_flags), (1, 0x10));
            assert_eq!(fields.level.as_ref().unwrap().level, 33);
            assert_eq!(
                fields.level.as_ref().unwrap().health,
                fields.level.as_ref().unwrap().maximum_health
            );
            assert_eq!(
                (
                    fields.faction,
                    fields.gossip,
                    fields.npc_flags,
                    fields.unit_flags,
                    fields.dynamic_flags
                ),
                (73, 0, 0, 0, 0)
            );
            assert_eq!(fields.attack_times, [2000; 3]);
            assert_eq!(fields.resistances, [0; 6]);
            assert!(!fields.invisible && !fields.ignore_mmap && !fields.count_spawns);
            assert!(fields.can_modify_stats && !fields.pvp);
            assert_eq!(template.words[89], 0);
            assert_eq!(fields.requested_spell_list, 469200);
            assert_ne!(fields.requested_spell_list, 0);
            assert_eq!(live.spell_list_disabled, Some(true));
            let live_list = live.spell_list.as_ref().unwrap();
            assert_eq!(Some(live_list), creature_spell_lists.get(469200));
            assert!(live_list.disabled);
            assert_eq!(live_list.spells.len(), 1);
            assert_eq!(live_list.spells[&0].spell_id, 5708);
            assert!(live_list
                .spells
                .values()
                .all(|spell| !spell.has_written_combat_condition()));
            assert!(!fields.chain_immune);
            assert_eq!(template.words[16], 1);
            assert!(creature_immunities.get(entry, 0).is_none());
            assert_eq!(live.immunities.set, u32::MAX);
            assert_eq!(&template.words[91..93], &[0, 0]);
            assert_eq!(fields.settings.flags, template.words[25..29]);
            assert_eq!(
                active_objects.visibility_override_bits(candidate).unwrap(),
                0
            );
            assert_eq!(creature_battleground, Some(false));
            assert!(![139, 1377].contains(&creature_zone.unwrap()));
            assert!(!creature_linking_trigger);
            assert!(links.lookup(entry, attempt.guid, slot as u32).is_none());
            assert!(!links.event_trigger(entry, attempt.guid, slot as u32));
            assert!(continents.maps[slot]
                .instance_data
                .as_ref()
                .unwrap()
                .stores
                .creatures
                .is_empty());
            assert_eq!(
                active_objects.pose(candidate).unwrap(),
                active_objects.get(candidate).unwrap().respawn_pose.unwrap()
            );
            assert!(active_objects.get(candidate).unwrap().in_world);
            println!(
                "F6_CREATURE_CREATE map={slot} static_flags={:?} unit_flags={} visibility_override_bits={} zone={creature_zone:?} linking_trigger={creature_linking_trigger}",
                fields.settings.flags,
                fields.unit_flags,
                active_objects.visibility_override_bits(candidate).unwrap()
            );
            assert_eq!(
                evidence.first_candidate_rng,
                [
                    ("irand".into(), 678, 0, 99, Some(93)),
                    ("urand".into(), 1279, 32, 33, Some(33)),
                    ("urand".into(), 313, 300, 300, Some(300)),
                    ("urand".into(), 23, 0, 0, Some(0)),
                    ("urand".into(), 23, 0, 0, Some(0)),
                    ("urand".into(), 47, 0, 0, Some(0)),
                    ("urand".into(), 47, 0, 0, Some(0)),
                    ("urand".into(), 40, 0, 10000, Some(5376))
                ]
            );
            assert_eq!(
                evidence.first_created_guid,
                Some((
                    60310195,
                    attempt.guid,
                    bound_guid.unwrap().counter(),
                    entry,
                    bound_guid.unwrap().high()
                ))
            );
            let object = candidate;
            assert_eq!(
                active_objects.get(object).unwrap().respawn_pose,
                Some(SourcePose(
                    owner
                        .spawns
                        .complete_data(SpawnKey {
                            kind: GridSpawnKind::Creature,
                            guid: attempt.guid
                        })
                        .unwrap()
                        .unwrap()
                        .row
                        .pose
                        .map(f32::to_bits)
                ))
            );
        }
        assert_eq!(
            evidence.first_candidate,
            Some((
                if slot == 0 { 60382119 } else { 60310149 },
                u32::from(attempt.kind == GridSpawnKind::GameObject),
                attempt.guid,
                attempt.cell.x(),
                attempt.cell.y(),
            ))
        );
        // :60982845 completes GUID12056, and :60649319 completes GUID42964.
        assert_eq!(completed, if slot == 0 { 429 } else { 281 });
        println!(
            "F6_VISIBILITY map={slot} census={visibility:?} invalidated_camera_iterator_stock_reach={}",
            visibility.cameras
        );
        println!(
            "F6_ACTIVE_PREFIX map={slot} first_grid={:?} candidate_guid={} entry={entry} candidate_allocations={candidates} completed_objects={completed} next_owner={}",
            evidence.first_grid, attempt.guid, refusal.id
        );
        assert_eq!(continents.maps[slot].graveyards, world_graveyards);
        assert_eq!(
            continents.maps[slot]
                .graveyards
                .entries()
                .values()
                .map(Vec::len)
                .sum::<usize>(),
            190
        );
        assert!(!continents.maps[slot].spawn_updated);
        assert_eq!(
            random.cursor().raw,
            if slot == 0 {
                MAP0_NEXT_RAW as u64
            } else {
                after_pools_cursor + 2031 // Last consumed word: trace :60650115.
            }
        );
        matched_map_words[slot] = random.cursor().raw;
        assert_eq!(after_pools_cursor as usize, evidence.pool_raw_end.unwrap());
        let state = continents.maps[slot].persistent.unwrap();
        assert_eq!(
            states.state(state).unwrap().map_binding(),
            Some(continents.maps[slot].identity)
        );
        assert!(states.state(state).unwrap().pools().initialized());
        assert!(!continents.maps[slot].spawn_updated);
        let inventory = pools.inventory();
        assert!(inventory.frames.is_empty() && !inventory.stopped);
        let (pool, kind) = owner
            .pool_operations
            .iter()
            .find_map(|operation| {
                if let PoolOperation::Roll {
                    state: reached,
                    pool,
                    kind,
                    ..
                } = operation
                {
                    (*reached == state).then_some((*pool, *kind))
                } else {
                    None
                }
            })
            .unwrap();
        let kind_index = match kind {
            PoolKind::Child => 0,
            PoolKind::GameObject => 1,
            PoolKind::Creature => 2,
        };
        let members = &inventory.templates[usize::from(pool)].groups[kind_index].equal;
        assert_eq!(
            members.iter().map(|member| member.guid).collect::<Vec<_>>(),
            evidence.shuffle_before
        );
        assert_eq!(shuffles.len(), evidence.pool_shuffles.unwrap());
        assert_eq!(
            shuffles[0]
                .iter()
                .map(|&index| members[index].guid)
                .collect::<Vec<_>>(),
            evidence.shuffle_after
        );
        let expected = serde_json::json!({
            "creatures": evidence.pool_members[0],
            "gameobjects": evidence.pool_members[1],
            "pools": evidence.pool_counts,
            "initialized": true,
        });
        assert_eq!(
            serde_json::to_value(states.state(state).unwrap().pools()).unwrap(),
            expected
        );
        println!(
            "F6_MAP_POOL thread={thread} matched_words={} matched_pool_members={} matched_pool_counts={} shuffles={} map_event_active=[] pool_snapshot_line={} event_initialize_line={event_initialize_line} graveyards=190 graveyard_init_words=0 graveyard_init_sql=0 variable_init_words=0 variable_init_sql=0 encounter_bounds={} next_owner={} next_source={}",
            matched_map_words[slot],
            evidence
                .pool_members
                .iter()
                .map(BTreeSet::len)
                .sum::<usize>(),
            evidence.pool_counts.len(),
            shuffles.len(),
            evidence.pool_snapshot_line.unwrap(),
            encounters.by_map(slot as u32).len(),
            refusal.id,
            if slot == 0 { "SpellAuras.cpp:4114" } else { "ScriptDevAIMgr.cpp:370" },
        );
        owner.states = states;
        let raw = trace
            .iter()
            .filter_map(|event: &TraceEvent| {
                if let EventKind::Raw { position, word, .. } = event.kind {
                    Some((position, word))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            &raw[..evidence.before_sql.len()],
            evidence
                .before_sql
                .iter()
                .map(|&(_, position, word)| (position, word))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            raw,
            evidence.words[..matched_map_words[slot] as usize]
                .iter()
                .map(|&(_, position, word)| (position, word))
                .collect::<Vec<_>>()
        );
        if slot == 0 {
            // Creature.h:313; TimerAI.cpp:23,47. CreatureEventAI::ProcessEvent
            // draws chance at :607 and the action word at :618, even for ordered actions.
            assert_eq!(
                &raw[27080..27087],
                &[
                    (27080, 3988355474),
                    (27081, 2106366240),
                    (27082, 1286149830),
                    (27083, 562578093),
                    (27084, 1588247119),
                    (27085, 985444834),
                    (27086, 73362909),
                ]
            );
            assert_eq!(evidence.words[27085], (60477731, 27085, 985444834));
            assert_eq!(evidence.words[27086], (60477753, 27086, 73362909));
        }
        if slot == 1 {
            // Recording60368585 is the first AI word after GUID28842's launch.
            assert_eq!(raw[12112], (12112, 257701202));
            assert_eq!(
                &raw[11898..11903],
                &[
                    (11898, 2088835978),
                    (11899, 2716685484),
                    (11900, 863995997),
                    (11901, 3633643894),
                    (11902, 2308746867),
                ]
            );
            relations_evidence::assert_azuregos_requests(&evidence.candidate_requests);
        }
        assert!(trace.iter().all(|event| event.stream == thread));
        assert_eq!(evidence.first_word, Some((first_line, 0, first_word)));
        {
            use benilla_sim::source_rng::{Operation, Value};
            let calls = trace
                .iter()
                .filter_map(|event| {
                    if let EventKind::Begin {
                        span,
                        parent: None,
                        at,
                        operation,
                        args,
                    } = event.kind
                    {
                        if at >= after_pools_cursor {
                            return Some((span, at, operation, args));
                        }
                    }
                    None
                })
                .collect::<Vec<_>>();
            assert_eq!(
                calls
                    .iter()
                    .map(|(_, at, operation, args)| (*at, *operation, *args))
                    .collect::<Vec<_>>(),
                evidence
                    .candidate_requests
                    .iter()
                    .filter(|request| (request.0 as u64) < matched_map_words[slot])
                    .map(|request| {
                        let operation = match request.1.as_str() {
                            "urand" => Operation::Urand,
                            "irand" => Operation::Irand,
                            "urand_default" => Operation::UrandDefault,
                            other => panic!("unported recorded RNG request {other}"),
                        };
                        (
                            request.0 as u64,
                            operation,
                            [request.2 as u64, request.3 as u64, 0],
                        )
                    })
                    .collect::<Vec<_>>()
            );
            let results = evidence
                .candidate_requests
                .iter()
                .filter(|request| (request.0 as u64) < matched_map_words[slot])
                .map(|request| match request.1.as_str() {
                    "urand" | "urand_default" => Value::U64(request.4.unwrap().try_into().unwrap()),
                    "irand" => Value::I64(request.4.unwrap()),
                    other => panic!("unported recorded RNG result {other}"),
                })
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), results.len());
            for ((span, _, _, _), expected) in calls.into_iter().zip(results) {
                assert!(trace.iter().any(|event| matches!(event.kind, EventKind::End { span: end_span, result: Ok(result), .. } if span == end_span && result == expected)));
            }
        }
        // Initial-state projection remains independent of the now-executed script draws.
        let mut initial_engine = Mt19937::from_state(evidence.initial.clone().unwrap()).unwrap();
        assert_eq!(u64::from(initial_engine.next_u32()), first_word);
    }
    if continents.maps.iter().any(|map| map.script_id != 0) {
        let refusal = refuse(active_init::CREATURE_NEXT_OWNER[0]);
        assert!(!events.inventory().initialized);
        assert!(owner.sql.is_empty() && owner.raw.is_empty());
        assert_eq!(owner.cursor.raw, 8712);
        assert_eq!(next_word, (8712, 3411341330));
        assert_eq!(u64::from(owner.engine.clone().next_u32()), next_word.1);
        assert_eq!(matched_map_sql, 4);
        let results = owner
            .pool_operations
            .iter()
            .filter(|operation| matches!(operation, PoolOperation::Selected { add: true, .. }))
            .count();
        assert!(results > 0);
        println!(
            "F6_SEAL16_STARTUP matched_spawn_selections=5324 matched_raw_words={} matched_map_thread_words={matched_map_words:?} matched_map_sql=4 matched_script_names_sql=1 matched_template_sql=6 matched_creature_model_sql=3 matched_equipment_sql=3 matched_creature_immunity_sql=1 matched_creature_spell_list_sql=5 matched_spawn_data_sql=2 matched_graveyard_sql=1 matched_encounter_sql=4 matched_group_sql=6 pool_results={results} event_initialization_reached=false next_owner_map0={} next_owner_map1={} next_raw={} next_word={} clone_next_word_checked=true",
            owner.cursor.raw,
            refusal.id,
            active_init::CREATURE_NEXT_OWNER[1],
            next_word.0,
            next_word.1
        );
        return;
    }
    let initial_status = owner.status.iter().copied().collect::<Vec<_>>();
    let announce = super::super::spawn_loader_config::assignment(&config, "Event.Announce")
        .unwrap()
        .1
        .parse::<u32>()
        .unwrap()
        != 0;
    let delay = events
        .initialize(
            (!initial_status.is_empty()).then_some(initial_status.as_slice()),
            announce,
            &mut owner,
        )
        .unwrap();
    if owner.sql != observed.expected_sql {
        let index = owner
            .sql
            .iter()
            .zip(&observed.expected_sql)
            .position(|(a, b)| a != b)
            .unwrap_or(owner.sql.len().min(observed.expected_sql.len()));
        println!(
            "F6_FIRST_DIVERGENCE event_sql_index={index} expected={:?} got={:?} raw_words={} candidate_causes=calendar_inputs,source_query_order,event_effect_order",
            observed.expected_sql.get(index),
            owner.sql.get(index),
            owner.cursor.raw
        );
    }
    assert_eq!(owner.sql, observed.expected_sql);
    assert_eq!(&owner.status, &events.inventory().active);
    assert_eq!(&owner.quests, &events.inventory().quest_active);
    assert_quest_activity(&owner);
    assert!(!events.active(27));
    assert_eq!(owner.loaded_maps.len(), observed.map_births);
    let created = persistent
        .operations
        .iter()
        .filter_map(|op| {
            if let PersistentOperation::Created { state } = op {
                Some((state.map, state.instance))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(created, observed.persistent_creations);
    assert!(events.active(400));
    assert!(owner.update.is_empty() && owner.checks.is_empty());
    if !owner.raw.is_empty() {
        println!(
            "F6_FIRST_DIVERGENCE raw_index={} expected_owner=WorldState.cpp:605 got_owner=PoolManager expected_word={} got_words={:?} candidate_causes=event_windows,source_query_order,pool_event_membership",
            next_word.0, next_word.1, owner.raw
        );
    }
    assert!(owner.raw.is_empty());
    assert_eq!(owner.cursor.raw, next_word.0);
    let next = u64::from(owner.engine.clone().next_u32());
    assert_eq!(next, next_word.1);
    let refusal = refuse("execution.world_state.startup_owner");
    println!(
        "F6_SEAL16_STARTUP matched_spawn_selections=5324 matched_raw_words={} event_definitions={} event7_clock_bound=true actual_sine_dispatch_bound=true event27_active=false pool_shuffles=0 pool_results=0 matched_status_writes={} persistent_states={} event_delay_ms={delay} next_raw={} next_word={next} next_owner={} next_source=WorldState.cpp:605",
        owner.cursor.raw,
        events.inventory().definitions.len(),
        owner.sql.len(),
        owner.states.inventories().len(),
        next_word.0,
        refusal.id
    );
}

#[test]
fn map_thread_evidence_keeps_raw_positions_and_pre_sql_boundaries_independent() {
    let mut inputs = Inputs::default();
    inputs.observe_thread(6, 11, "mt_raw", &["engine6", "0", "42"]);
    inputs.observe_thread(
        7,
        12,
        "sql_request_bytes",
        &["db", "0", "0", "1", "0", "42"],
    );
    inputs.observe_thread(7, 13, "mt_raw", &["engine7", "0", "19"]);
    inputs.observe_thread(6, 14, "mt_raw", &["engine6", "1", "99"]);
    inputs.observe_thread(
        6,
        15,
        "sql_request_bytes",
        &["db", "0", "0", "1", "0", "41"],
    );
    inputs.observe_thread(6, 16, "mt_raw", &["engine6", "2", "3"]);
    assert_eq!(
        inputs.map_threads[&6].before_sql,
        [(11, 0, 42), (14, 1, 99)]
    );
    assert!(inputs.map_threads[&7].before_sql.is_empty());
    assert_eq!(inputs.map_threads[&6].first_word, Some((11, 0, 42)));
    assert_eq!(inputs.map_threads[&7].first_word, Some((13, 0, 19)));
    assert_eq!(inputs.map_threads[&6].total_words, 3);
    assert_eq!(inputs.map_threads[&7].total_words, 1);
    assert_eq!(inputs.map_threads[&6].sql_line, Some(15));
    assert_eq!(inputs.map_threads[&7].sql_line, Some(12));
}

#[test]
fn initialize_clock_binding_keeps_map_threads_and_callsite_identity_separate() {
    let mut inputs = Inputs::default();
    let site = [
        "1",
        "3678789157770927477",
        "7287768",
        "/recorded/bin/mangosd",
    ];
    inputs.observe_thread(6, 10, "clock_callsite", &site);
    inputs.observe_thread(7, 11, "clock_time", &["100", "ptr"]);
    assert!(inputs.map_threads[&7].spawn_clock.is_none());
    inputs.observe_thread(6, 12, "clock_time", &["101", "ptr"]);
    assert_eq!(inputs.map_threads[&6].spawn_clock, Some((12, 101, 0)));
    inputs.observe_thread(
        7,
        13,
        "clock_callsite",
        &["1", "wrong-image", "7287768", "/recorded/bin/mangosd"],
    );
    inputs.observe_thread(7, 14, "clock_time", &["102", "ptr"]);
    assert!(inputs.map_threads[&7].spawn_clock.is_none());
    inputs.observe_thread(7, 15, "clock_callsite", &site);
    inputs.observe_thread(7, 16, "clock_time", &["103", "ptr"]);
    assert_eq!(inputs.map_threads[&7].spawn_clock, Some((16, 103, 0)));
}

#[test]
fn candidate_rng_evidence_retains_full_width_calls_and_range_bounds() {
    let mut inputs = Inputs::default();
    inputs.map_threads.entry(7).or_default().first_candidate = Some((1, 0, 1, 0, 0));
    inputs.observe_thread(
        7,
        10,
        "rng_call",
        &["urand_default", "CreatureEventAI.cpp", "607"],
    );
    inputs.observe_thread(7, 11, "mt_raw", &["engine7", "0", "42"]);
    inputs.observe_thread(7, 12, "rng_result", &["urand_default", "42"]);
    inputs.observe_thread(7, 13, "rng_call", &["irand", "source.cpp", "23", "-2", "7"]);
    inputs.observe_thread(7, 14, "mt_raw", &["engine7", "1", "67"]);
    inputs.observe_thread(7, 15, "rng_result", &["irand", "6"]);
    assert_eq!(
        inputs.map_threads[&7].candidate_requests,
        [
            (0, "urand_default".into(), 0, 0, Some(42)),
            (1, "irand".into(), -2, 7, Some(6))
        ]
    );
    assert_eq!(
        inputs.map_threads[&7].first_candidate_rng,
        [
            ("urand_default".into(), 607, 0, 0, Some(42)),
            ("irand".into(), 23, -2, 7, Some(6))
        ]
    );
}
