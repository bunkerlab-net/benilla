// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent seal16 trace lines, used only as assertions after execution.
use super::{
    unhex, CreatureAddonAuras, CreatureAddonTable, GridSpawnKind, HighGuid, LoadedCreatureAddons,
    LoadedCreatureTemplates, ObjectHandle, SourceContinent, SourceDynamicModels,
    SourceEffectiveSpawns, SourceObjectKind, SourceObjects, SourceSpellStores, SourceVMapManager,
    SpellTargetImplicitType, TransportTemplateEvent, SPELL_EFFECT_INFO_TABLE,
};
use std::collections::BTreeSet;

pub(super) fn assert_world_boss_reach(
    templates: &LoadedCreatureTemplates,
    spawns: &SourceEffectiveSpawns,
) {
    // initial-classicmangos.sql:421041 (`Rank`), :80423-80424 (`id`, `map`).
    let boss_entries: BTreeSet<_> = templates
        .rows()
        .iter()
        .filter(|row| row.words[37] == 3)
        .map(|row| row.words[0])
        .collect();
    let mut by_map = [0; 2];
    for (key, spawn) in spawns.pre_terrain_map() {
        if key.kind == GridSpawnKind::Creature
            && spawn.row.map < 2
            && boss_entries.contains(&spawn.row.entry)
        {
            by_map[spawn.row.map as usize] += 1;
        }
    }
    assert_eq!((boss_entries.len(), by_map), (153, [8, 13]));
}

pub(super) fn assert_plaguelands_creature_census(spawns: &SourceEffectiveSpawns) {
    let mut counts = [0; 6];
    for (key, spawn) in spawns.pre_terrain_map() {
        if key.kind != GridSpawnKind::Creature || spawn.row.map != 0 {
            continue;
        }
        let spawn = spawns.complete_data(*key).unwrap().unwrap();
        if spawn.original_zone().unwrap() == 139 {
            counts[match spawn.validation_entry {
                17209 => 1,
                17635 => 2,
                17647 => 3,
                17995 => 4,
                17996 => 5,
                _ => 0,
            }] += 1;
        }
    }
    // Stock creature and alternative rows contain none of the five special entries.
    assert_eq!(&counts[1..], &[0; 5]);
    println!(
        "F6_EP_CREATURE_CENSUS zone=139 default={} flight_master={} soldiers={:?}",
        counts[0],
        counts[1],
        &counts[2..]
    );
}

pub(super) fn assert_garments_reach(spawns: &SourceEffectiveSpawns, addons: &LoadedCreatureAddons) {
    // initial-classicmangos.sql :429338, :429341-429344.
    let entries = [12423, 12427, 12428, 12429, 12430];
    let rows = spawns
        .pre_terrain_map()
        .iter()
        .filter(|(key, spawn)| {
            key.kind == GridSpawnKind::Creature && entries.contains(&spawn.row.entry)
        })
        .map(|(key, spawn)| {
            // SQL :437897 and :437899-437902. No stock Reset changes the stand byte.
            assert_eq!(
                addons
                    .get(HighGuid::Creature, key.guid, spawn.row.entry)
                    .unwrap()
                    .stand_state,
                8
            );
            (key.guid, spawn.row.entry, spawn.row.map)
        })
        .collect::<Vec<_>>();
    // SQL :80645, :83660, :118956, :120782, :126198.
    assert_eq!(
        rows,
        [
            (224, 12427, 0),
            (3431, 12430, 1),
            (44557, 12428, 0),
            (46717, 12429, 1),
            (81085, 12423, 0)
        ]
    );
}

pub(super) fn assert_static_area_aura_reach(
    addons: &LoadedCreatureAddons,
    stores: &SourceSpellStores,
) {
    let spells: BTreeSet<_> = [CreatureAddonTable::Template, CreatureAddonTable::Guid]
        .into_iter()
        .flat_map(|table| addons.table_rows(table))
        .filter_map(|row| match &row.auras {
            CreatureAddonAuras::Spells(spells) => Some(spells),
            CreatureAddonAuras::Empty => None,
            CreatureAddonAuras::Raw(_) => {
                panic!("addon rows must finish loading before the census")
            }
        })
        .flatten()
        .copied()
        .filter(|&spell| spell != 0)
        .collect();
    assert_eq!(spells.len(), 262);
    let mut counts = [0; 4];
    for id in spells {
        let effects = stores.template(id).unwrap().effects();
        counts[0] += usize::from(effects.iter().any(|e| {
            SPELL_EFFECT_INFO_TABLE[e.effect as usize].required_target
                == SpellTargetImplicitType::SpecialUnit
        }));
        counts[1] += usize::from(effects.iter().any(|e| e.effect == 35));
        counts[2] += usize::from(effects.iter().any(|e| e.effect != 0 && e.aura == 22));
        counts[3] += usize::from(effects.iter().any(|e| e.effect == 35 && e.aura == 22));
    }
    // initial-classicmangos.sql:146910-148699,431520-438994,837821-860203.
    assert_eq!(counts, [10, 9, 15, 3]);
    println!(
        "F6_ADDON_AREA_STATIC distinct=262 special_unit={} effect_35={} aura_22={} effect_35_aura_22={}",
        counts[0], counts[1], counts[2], counts[3]
    );
}

pub(super) const COMPLETED: [(u64, u32); 27] = [
    (60413544, 27677),
    (60414358, 28094),
    (60415105, 27008),
    (60415969, 29195),
    (60416726, 28080),
    (60417751, 27029),
    (60418476, 26999),
    (60419416, 29004),
    (60420284, 28229),
    (60421055, 29164),
    (60421732, 29022),
    (60422397, 28113),
    (60423043, 28913),
    (60423923, 28118),
    (60424699, 26998),
    (60425386, 28095),
    (60426183, 29001),
    (60426970, 27009),
    (60427629, 27006),
    (60428459, 28085),
    (60429228, 28909),
    (60429948, 28994),
    (60430771, 28106),
    (60431375, 27007),
    (60431932, 29020),
    (60432544, 28082),
    (60433318, 32518),
];

pub(super) fn assert_words(words: &[(u64, u64, u64)]) {
    assert_eq!(
        &words[12422..12623],
        &[
            (60412568, 12422, 3973391667),
            (60413336, 12423, 2055897542),
            (60413355, 12424, 3900825516),
            (60413401, 12425, 4234510177),
            (60413419, 12426, 138510493),
            (60413444, 12427, 914297211),
            (60413471, 12428, 1048520102),
            (60413497, 12429, 4081018766),
            (60413567, 12430, 3836430285),
            (60413577, 12431, 712386203),
            (60413699, 12432, 2171997182),
            (60414205, 12433, 1679299104),
            (60414225, 12434, 4086422405),
            (60414275, 12435, 17310626),
            (60414295, 12436, 1992084187),
            (60414319, 12437, 3231666802),
            (60414403, 12438, 370366380),
            (60414425, 12439, 1176268098),
            (60414552, 12440, 77532561),
            (60415040, 12441, 4132968264),
            (60415050, 12442, 1903080311),
            (60415075, 12443, 1594291468),
            (60415085, 12444, 228254554),
            (60415125, 12445, 684088708),
            (60415218, 12446, 4050210331),
            (60415848, 12447, 3792858042),
            (60415868, 12448, 3971443679),
            (60415920, 12449, 2701215335),
            (60415940, 12450, 201090555),
            (60416012, 12451, 1220512694),
            (60416033, 12452, 1663054305),
            (60416165, 12453, 2533434862),
            (60416633, 12454, 997853249),
            (60416643, 12455, 534977105),
            (60416670, 12456, 449188826),
            (60416682, 12457, 2060380677),
            (60416694, 12458, 202527422),
            (60416774, 12459, 266305438),
            (60416796, 12460, 155164110),
            (60416941, 12461, 286111377),
            (60417577, 12462, 2508049391),
            (60417596, 12463, 746117286),
            (60417638, 12464, 2412999170),
            (60417655, 12465, 785951133),
            (60417675, 12466, 3870328470),
            (60417694, 12467, 1186569178),
            (60417714, 12468, 1583074600),
            (60417786, 12469, 1206593045),
            (60417800, 12470, 447189013),
            (60417916, 12471, 1300530573),
            (60418355, 12472, 223272827),
            (60418371, 12473, 135823992),
            (60418416, 12474, 2936685992),
            (60418435, 12475, 1187472000),
            (60418530, 12476, 231885896),
            (60418549, 12477, 3319381361),
            (60418690, 12478, 4150160967),
            (60419291, 12479, 296881007),
            (60419304, 12480, 3241173478),
            (60419345, 12481, 1516823915),
            (60419360, 12482, 3644638838),
            (60419378, 12483, 223669605),
            (60419462, 12484, 502457193),
            (60419484, 12485, 2581066632),
            (60419571, 12486, 1224607643),
            (60420180, 12487, 234010303),
            (60420202, 12488, 1536492000),
            (60420244, 12489, 1290432551),
            (60420254, 12490, 1726221521),
            (60420264, 12491, 2446585383),
            (60420304, 12492, 1497759790),
            (60420375, 12493, 366726276),
            (60420947, 12494, 803815726),
            (60420969, 12495, 4076789641),
            (60421010, 12496, 4173294180),
            (60421023, 12497, 1726359360),
            (60421089, 12498, 1528677812),
            (60421103, 12499, 1385586132),
            (60421216, 12500, 4044612606),
            (60421609, 12501, 4126339839),
            (60421627, 12502, 2074322002),
            (60421668, 12503, 3440101287),
            (60421685, 12504, 17939637),
            (60421704, 12505, 1589632521),
            (60421765, 12506, 541328201),
            (60421784, 12507, 1965390466),
            (60421906, 12508, 142192180),
            (60422280, 12509, 1795788857),
            (60422296, 12510, 1669535662),
            (60422338, 12511, 1738611536),
            (60422353, 12512, 3864446503),
            (60422363, 12513, 3629116333),
            (60422424, 12514, 313637644),
            (60422444, 12515, 2041195759),
            (60422558, 12516, 1752742656),
            (60422928, 12517, 1837643715),
            (60422947, 12518, 4122301689),
            (60422985, 12519, 1393678188),
            (60423003, 12520, 477330521),
            (60423094, 12521, 186348344),
            (60423116, 12522, 3258916560),
            (60423244, 12523, 1980319573),
            (60423813, 12524, 834324898),
            (60423835, 12525, 3035836614),
            (60423882, 12526, 3775350300),
            (60423893, 12527, 2176131429),
            (60423903, 12528, 926277615),
            (60423943, 12529, 199274437),
            (60423953, 12530, 2298927056),
            (60424020, 12531, 1947700407),
            (60424610, 12532, 204550295),
            (60424621, 12533, 180327534),
            (60424655, 12534, 1983836401),
            (60424670, 12535, 2021374015),
            (60424752, 12536, 3223497985),
            (60424773, 12537, 165658043),
            (60424896, 12538, 2661915224),
            (60425259, 12539, 3532409533),
            (60425277, 12540, 1983475548),
            (60425326, 12541, 3422624089),
            (60425337, 12542, 1588762945),
            (60425358, 12543, 921381422),
            (60425433, 12544, 2558633926),
            (60425455, 12545, 2031985863),
            (60425578, 12546, 860038156),
            (60426087, 12547, 2641619574),
            (60426102, 12548, 1696677149),
            (60426143, 12549, 3367836277),
            (60426153, 12550, 1799112197),
            (60426163, 12551, 4136673121),
            (60426203, 12552, 3320651035),
            (60426213, 12553, 555413645),
            (60426278, 12554, 1581151936),
            (60426865, 12555, 875558556),
            (60426882, 12556, 2360143928),
            (60426916, 12557, 1772746577),
            (60426935, 12558, 802538638),
            (60427013, 12559, 2597105585),
            (60427035, 12560, 1008749387),
            (60427146, 12561, 3144743955),
            (60427510, 12562, 797677302),
            (60427527, 12563, 1844895947),
            (60427565, 12564, 2723965318),
            (60427586, 12565, 676669110),
            (60427673, 12566, 455307895),
            (60427694, 12567, 1541735738),
            (60427828, 12568, 2948389885),
            (60428351, 12569, 1329130086),
            (60428371, 12570, 2258192600),
            (60428411, 12571, 3029751300),
            (60428429, 12572, 819748154),
            (60428439, 12573, 1408021376),
            (60428479, 12574, 441549373),
            (60428489, 12575, 2881041926),
            (60428555, 12576, 2750157897),
            (60429105, 12577, 4001925706),
            (60429120, 12578, 1856092957),
            (60429165, 12579, 2835643221),
            (60429187, 12580, 1573257243),
            (60429266, 12581, 1007330564),
            (60429277, 12582, 1932033699),
            (60429372, 12583, 1253000387),
            (60429873, 12584, 3312427290),
            (60429883, 12585, 4120522372),
            (60429908, 12586, 1561526997),
            (60429918, 12587, 3620366354),
            (60429928, 12588, 1105798827),
            (60429968, 12589, 3580673986),
            (60429978, 12590, 4714473),
            (60430067, 12591, 1560249131),
            (60430636, 12592, 2251402487),
            (60430658, 12593, 993484998),
            (60430697, 12594, 1445504081),
            (60430718, 12595, 2875027488),
            (60430738, 12596, 2995340531),
            (60430809, 12597, 1316495263),
            (60430825, 12598, 2601628693),
            (60430927, 12599, 3766434410),
            (60431310, 12600, 1674843442),
            (60431320, 12601, 1873556933),
            (60431345, 12602, 4014487958),
            (60431355, 12603, 3617565866),
            (60431395, 12604, 261953771),
            (60431405, 12605, 3270262360),
            (60431468, 12606, 2494270941),
            (60431806, 12607, 3446653443),
            (60431820, 12608, 2323193842),
            (60431862, 12609, 2361296),
            (60431882, 12610, 2838923289),
            (60431896, 12611, 217312180),
            (60431972, 12612, 3946656754),
            (60431992, 12613, 3470623036),
            (60432099, 12614, 1314719349),
            (60432451, 12615, 4154675562),
            (60432461, 12616, 1916453308),
            (60432488, 12617, 1298327508),
            (60432499, 12618, 2484510439),
            (60432512, 12619, 415598489),
            (60433023, 12620, 3326989745),
            (60433384, 12621, 3103005066),
            (60433394, 12622, 1799347086),
        ]
    );
    assert_eq!(words[12623], (60433815, 12623, 1799183854));
}

pub(super) fn assert_continuation_words(words: &[(u64, u64, u64)]) {
    // seal16-full-01 trace.tsv, thread 7, lines 60434830 through 60534410.
    // Hash each (line, position, value) as three little-endian u64s.
    assert_eq!(
        digest(
            words[12624..13314]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "e73b69148f9c5c58d05e6fb117e76daef26b34b198f291b9848d22260427aa9f"
    );
    // Thread 7 continues with GUID50942, GUID50950, then GUID35936 before addon spell7276.
    assert_eq!(
        &words[13314..13332],
        &[
            (60534490, 13314, 3835546177),
            (60534505, 13315, 1278552396),
            (60534608, 13316, 370396801),
            (60535041, 13317, 1331250111),
            (60535056, 13318, 1805382002),
            (60535091, 13319, 260078945),
            (60535107, 13320, 996768929),
            (60535121, 13321, 311385393),
            (60535181, 13322, 3064099018),
            (60535196, 13323, 2116357069),
            (60535312, 13324, 3846255735),
            (60535346, 13325, 3225382514),
            (60535360, 13326, 1782421205),
            (60535396, 13327, 2278203877),
            (60535411, 13328, 2737314857),
            (60535426, 13329, 1851702798),
            (60535494, 13330, 1936796533),
            (60539844, 13331, 675875699),
        ]
    );
    // Creature.h:313 draws the next word only after the addon returns.
    assert_eq!(words[13332], (60539979, 13332, 3820596846));
    // Seal16 trace.tsv: 107 new (line, position, word) tuples, little-endian u64s.
    assert_eq!(
        digest(
            words[13332..13439]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "1099965c62ad0e7ccde116769ac7f40109fb18a36a1dbb74ae9502eb76a85a69"
    );
    assert_eq!(words[13438], (60553204, 13438, 3952305824));
    assert_eq!(words[13439], (60553836, 13439, 155414054));
    // Trace :60553836-:60601822, including the boss model roll at Creature.cpp:678.
    assert_eq!(
        digest(
            words[13439..13730]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "2c24ea7a5ce93df931dac3397d0db656c9de31ff0dbcd81c68438b880bb5b537"
    );
    assert_eq!(words[13729], (60601822, 13729, 2156260723));
    assert_eq!(words[13730], (60602255, 13730, 435216370));
    // Trace :60602255-:60616628, including both world-boss respawn draws.
    assert_eq!(
        digest(
            words[13730..13783]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "c8a2c511760ce33447a612bd229cc677baac6a6a3b2fa79921ecd3eee66e887c"
    );
    // UnitAI 2000/2001, CombatAI 1000/1001, then Azuregos 0..5.
    // Reset draws all six combat timers first, then custom timers by ID.
    assert_eq!(
        &words[13783..13803],
        &[
            (60622967, 13783, 2606266200),
            (60622982, 13784, 3545480008),
            (60622997, 13785, 2831756592),
            (60623010, 13786, 712756487),
            (60623022, 13787, 3399081992),
            (60623036, 13788, 2963348438),
            (60623049, 13789, 2721552185),
            (60623062, 13790, 862859982),
            (60623073, 13791, 4002594707),
            (60623085, 13792, 4104714578),
            (60623123, 13793, 3513295978),
            (60623136, 13794, 2966504582),
            (60623153, 13795, 2958419712),
            (60623173, 13796, 1100549311),
            (60623184, 13797, 3232642378),
            (60623194, 13798, 2178491331),
            (60623213, 13799, 1170659765),
            (60623231, 13800, 252024332),
            (60623249, 13801, 1534314507),
            (60623267, 13802, 1898325816),
        ]
    );
    // Trace :60623350-:60650115, through Nelson's respawn draw.
    assert_eq!(
        digest(
            words[13803..13926]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "219ce482cd14470bbbcaa680e1e591f94a0bae7eb8940fafbafe8a1db5891194"
    );
    // :60657555-:60672859 are the four base timer words. The next two
    // creatures consume :60687871-:60690768 before the map reaches Wind Stone.
    assert_eq!(
        &words[13926..13947],
        &[
            (60657555, 13926, 1359723292),
            (60657575, 13927, 2421044289),
            (60672839, 13928, 508077146),
            (60672859, 13929, 331633868),
            (60687871, 13930, 1097510118),
            (60688178, 13931, 505139140),
            (60688329, 13932, 1604461379),
            (60689305, 13933, 1026565441),
            (60689327, 13934, 3622726339),
            (60689380, 13935, 1334046985),
            (60689392, 13936, 3156293777),
            (60689412, 13937, 242077266),
            (60689432, 13938, 3044639056),
            (60689519, 13939, 3362496960),
            (60689534, 13940, 2425180166),
            (60689743, 13941, 174389893),
            (60689894, 13942, 3443532078),
            (60690676, 13943, 25101090),
            (60690696, 13944, 3900081597),
            (60690748, 13945, 1292245421),
            (60690768, 13946, 4203303550),
        ]
    );
    // :60691067-:60741222. Wind Stone respawn through the geyser respawn draw.
    assert_eq!(
        digest(
            words[13947..14234]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "7abd0b2a60f636106788c11d17907487d809985d38fdd91bbe65c797f50a7f8c"
    );
    // :60741405-:60881944. Last word is Fire Shield's EventAI action roll.
    assert_eq!(
        digest(
            words[14234..14731]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "cafec04132efe462d271aede3297f22d745989429183fb9fede6adf6efe2b8b0"
    );
    // :60882507-:60901612. The last word is Hide's spell cooldown draw.
    assert_eq!(
        digest(
            words[14731..14857]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "06e7dba5933434d35518f4fe111e39c665be9faef1f1bece33ce0a1d72b360eb"
    );
    // :60901717-:60940782. Hide's respawn draw through the Outrider gender draw.
    assert_eq!(
        digest(
            words[14857..15080]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "0b61813af5b8865d87c23d05b626c951d37e6e4bb53c71df3fd29db9bcf29de6"
    );
    // :60941401-:61049072. The Outrider respawn through the last EventAI timer.
    assert_eq!(
        digest(
            words[15080..15783]
                .iter()
                .map(|&(line, raw, word)| [line, raw, word])
        ),
        "53acabfa052b5353814a735f2d1f91886149aa77e4c2d646cf978a707df36e4c"
    );
}

pub(super) fn assert_azuregos_requests(requests: &[(usize, String, i64, i64, Option<i64>)]) {
    let actual = requests
        .iter()
        .filter(|request| (13783..13803).contains(&request.0))
        .map(|(index, kind, min, max, result)| {
            assert_eq!(kind, "urand");
            (*index, *min, *max, result.unwrap())
        })
        .collect::<Vec<_>>();
    // TimerAI.cpp:23. UnitAI.cpp:60-61, CombatAI.cpp:33-34, boss_azuregos.cpp:57-62.
    assert_eq!(
        &actual[..10],
        &[
            (13783, 0, 0, 0),
            (13784, 0, 0, 0),
            (13785, 0, 0, 0),
            (13786, 0, 0, 0),
            (13787, 5000, 17000, 14497),
            (13788, 10000, 30000, 23799),
            (13789, 2000, 8000, 5802),
            (13790, 30000, 30000, 30000),
            (13791, 15000, 30000, 28979),
            (13792, 7000, 7000, 7000),
        ]
    );
    // TimerAI.cpp:47,146-150,182-194. Combat IDs 0..5, custom IDs 1000..1001, 2000..2001.
    assert_eq!(
        &actual[10..],
        &[
            (13793, 5000, 17000, 14816),
            (13794, 10000, 30000, 23814),
            (13795, 2000, 8000, 6133),
            (13796, 30000, 30000, 30000),
            (13797, 15000, 30000, 26290),
            (13798, 7000, 7000, 7000),
            (13799, 0, 0, 0),
            (13800, 0, 0, 0),
            (13801, 0, 0, 0),
            (13802, 0, 0, 0),
        ]
    );
}

pub(super) fn assert_continuation_clocks(
    before: &super::active_init::ActiveClocks,
    after: &super::active_init::ActiveClocks,
) {
    let consumed = before.0.iter().take(before.0.len() - after.0.len());
    assert_eq!(consumed.clone().count(), 535); // :60941614-:61048707 adds 96 clocks.
    assert!(after.0.is_empty());
    assert_eq!(
        digest(before.0.range(439..535).map(|sample| [
            sample.site_line,
            sample.time_line,
            sample.thread,
            sample.site[2],
            sample.raw,
            sample.seconds as u64,
        ])),
        "dfd4e3f2bf14c54dfa8f8df14f4e1ff9a92bec485cb52818e44dbfb04c0a4187"
    );
    let suffix = consumed.filter(|sample| sample.time_line >= 60434041);
    let prefix = suffix
        .clone()
        .take_while(|sample| sample.time_line <= 60534324);
    assert_eq!(prefix.clone().count(), 93);
    // Seal16 trace.tsv: clocks from 7165's holder through 590005's respawn.
    assert_eq!(
        digest(prefix.map(|sample| [
            sample.site_line,
            sample.time_line,
            sample.thread,
            sample.site[2],
            sample.raw,
            sample.seconds as u64,
        ])),
        "71da92e362a0fca32f900af25078eb01ff76df5a69aaa6e1767af070f4008006"
    );
    assert_eq!(
        suffix
            .clone()
            .filter(|sample| (60534325..=60553741).contains(&sample.time_line))
            .map(|s| (
                s.site_line,
                s.time_line,
                s.thread,
                s.site[2],
                s.raw,
                s.seconds
            ))
            .collect::<Vec<_>>(),
        [
            (60534620, 60534621, 7, 10166329, 13317, 1794314190),
            (60535324, 60535325, 7, 10166329, 13325, 1794314190),
            (60539951, 60539954, 7, 8250845, 13332, 1794314190),
            (60539962, 60539964, 7, 8249833, 13332, 1794314190),
            (60539994, 60539995, 7, 10166329, 13333, 1794314190),
            (60541147, 60541148, 7, 8250845, 13342, 1794314190),
            (60541150, 60541152, 7, 8249833, 13342, 1794314190),
            (60541168, 60541169, 7, 10166329, 13343, 1794314190),
            (60542768, 60542769, 7, 10166329, 13355, 1794314190),
            (60543785, 60543787, 7, 10166329, 13365, 1794314190),
            (60544369, 60544370, 7, 10166329, 13373, 1794314190),
            (60544932, 60544933, 7, 10166329, 13381, 1794314190),
            (60545775, 60545777, 7, 8250845, 13387, 1794314190),
            (60545849, 60545851, 7, 8249833, 13387, 1794314190),
            (60546490, 60546492, 7, 10166329, 13390, 1794314190),
            (60547784, 60547786, 7, 10166329, 13398, 1794314190),
            (60549132, 60549134, 7, 10166329, 13406, 1794314190),
            (60550187, 60550188, 7, 10166329, 13413, 1794314190),
            (60551589, 60551591, 7, 10166329, 13420, 1794314190),
            (60552233, 60552235, 7, 10166329, 13427, 1794314190),
            (60553010, 60553012, 7, 10166329, 13434, 1794314190),
            // Spell.cpp:1445 reaches the holder clock before the area effect dispatch.
            (60553739, 60553741, 7, 8250845, 13439, 1794314190),
        ]
    );
    // Trace :60553823-:60601741, from the area effect through GUID35127's last aura.
    let after_holder = suffix
        .clone()
        .filter(|sample| (60553742..=60601741).contains(&sample.time_line));
    assert_eq!(after_holder.clone().count(), 52);
    assert_eq!(
        digest(after_holder.map(|s| [
            s.site_line,
            s.time_line,
            s.thread,
            s.site[2],
            s.raw,
            s.seconds as u64
        ])),
        "db1a30877479d1d1f80f3f3ed4259a23cbb9e089b241a09198c513006ee1d0b8"
    );
    // Trace :60602283-:60616722, eight respawn reads and two aura reads.
    let boss_clocks = suffix
        .clone()
        .filter(|sample| (60601742..=60616722).contains(&sample.time_line));
    assert_eq!(boss_clocks.clone().count(), 10);
    assert_eq!(
        digest(boss_clocks.map(|s| [
            s.site_line,
            s.time_line,
            s.thread,
            s.site[2],
            s.raw,
            s.seconds as u64,
        ])),
        "13b92a61a0af6467b6641f6bdf24f1c9d54fadb7228147137627ee272ef5ab86"
    );
    // Trace :60623940-:60650167, 15 respawn clocks and two aura clocks.
    let after_azuregos = suffix
        .clone()
        .filter(|sample| (60616723..=60650167).contains(&sample.time_line));
    assert_eq!(after_azuregos.clone().count(), 17);
    assert_eq!(
        digest(after_azuregos.map(|s| [
            s.site_line,
            s.time_line,
            s.thread,
            s.site[2],
            s.raw,
            s.seconds as u64
        ])),
        "bc2287d69d00dd20cb7c59b2d45561816bc07cc039013e762d7aec6467bb4a43"
    );
    assert_eq!(
        suffix
            .clone()
            .filter(|sample| (60650168..60692060).contains(&sample.time_line))
            .map(|s| (s.site_line, s.time_line, s.raw, s.seconds))
            .collect::<Vec<_>>(),
        [
            (60657679, 60657681, 13928, 1794314190),
            (60672952, 60672954, 13930, 1794314191),
            (60688400, 60688402, 13933, 1794314191),
            (60689955, 60689956, 13943, 1794314191),
        ]
    );
    assert_eq!(
        digest(
            suffix
                .clone()
                .filter(|s| (60692060..=60739738).contains(&s.time_line))
                .map(|s| [s.site_line, s.time_line, s.raw, s.seconds as u64])
        ),
        "e46e21725f19c8d402a29d18b6879d0bc5cb2f52b3f36bd07792ba95e74b6662"
    );
    assert_eq!(
        digest(
            suffix
                .clone()
                .filter(|s| (60739739..=60857928).contains(&s.time_line))
                .map(|s| [s.site_line, s.time_line, s.raw, s.seconds as u64])
        ),
        "f5a5aa853ee325aed34715e0e71e617a531ee3d3079140215a9e29bcecf2f9e4"
    );
    // Fire Shield's holder and effect are :60882189 / :60882369.
    // Hide's holder and effect are :60901688 / :60901711.
    assert_eq!(
        digest(
            suffix
                .clone()
                .filter(|s| (60882189..=60901711).contains(&s.time_line))
                .map(|s| [s.site_line, s.time_line, s.raw, s.seconds as u64])
        ),
        "e870f5f93bffaf9dc4d4307f2b650f683553d1098f7b70ada61c9749f7b5c880"
    );
    // :60901724-:60939995. The first new clock is the Lurker's respawn read.
    assert_eq!(
        digest(
            suffix
                .filter(|s| (60901724..=60939995).contains(&s.time_line))
                .map(|s| [s.site_line, s.time_line, s.site[2], s.raw, s.seconds as u64])
        ),
        "753d834c0f4afa08d1fd31910260c3a71a6f0ed0fa70b10520ff19c94dd57900"
    );
}

pub(super) fn assert_after_area_cast(births: &[u64], completed: &[(u64, u32)]) {
    assert_eq!(
        &births[265..272],
        [60608842, 60610167, 60611466, 60612904, 60614131, 60615221, 60616138]
    );
    assert_eq!(
        &completed[157..164],
        [
            (60608833, 35867),
            (60610158, 36328),
            (60611459, 35699),
            (60612891, 35770),
            (60614125, 35701),
            (60615215, 35150),
            (60616132, 35783),
        ]
    );
    assert_eq!(
        &births[272..295],
        [
            60623311, 60624655, 60626036, 60627081, 60628108, 60630599, 60631578, 60633684,
            60634885, 60636075, 60644570, 60645999, 60646057, 60647493, 60647527, 60648417,
            60648446, 60649324, 60649360, 60687834, 60689479, 60690806, 60690842,
        ]
    );
    assert_eq!(
        &completed[164..182],
        [
            (60623298, 52349),
            (60624638, 35761),
            (60626020, 36343),
            (60627072, 35179),
            (60628106, 36338),
            (60630567, 35105),
            (60631566, 36317),
            (60633672, 35154),
            (60634874, 36329),
            (60636026, 35174),
            (60644555, 44205),
            (60645992, 43018),
            (60647488, 43019),
            (60648414, 42957),
            (60649319, 42964),
            (60687826, 43224),
            (60689472, 44359),
            (60690801, 43017),
        ]
    );
    // :60691109-:60740668 allocate 58 additional objects.
    assert_eq!(births.len(), 571);
    assert_eq!(
        digest(births[295..353].iter().map(|&line| [line])),
        "6a598f615159fe3b0f5cdab8ee894c46b3ef0de9a4dd31f7082e3fa9e4582319"
    );
    // :60691102-:60740587 complete 33 additional objects.
    assert_eq!(completed.len(), 429);
    assert_eq!(
        digest(
            completed[182..215]
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "00b000b3283dab5bb2919f90ba2de195b5db6a9c83de2091c0a1c39ba60cd3d2"
    );
    // :60741347-:60857633 adds 68 allocations, ending at GUID33193.
    assert_eq!(
        digest(births[353..421].iter().map(|&line| [line])),
        "a0e5084041e52f067f5a61ec9c282b622cec4035ef6920eac7185a7ed0f4cdc3"
    );
    // :60741339-:60857588 adds 63 completions, ending at GUID43735.
    assert_eq!(
        digest(
            completed[215..278]
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "6997f91d1cd8987749c93df0448cb6b015b6fb09f84248c685722d55d678e6d9"
    );
    // :60882448-:60901026 allocates 18 objects through GUID34917.
    assert_eq!(
        digest(births[421..439].iter().map(|&line| [line])),
        "3e9f51eb78f30503a978d46c317be2510399a9c7ff0090b06d80bd33b3bc439e"
    );
    // :60882413-:60901023 completes 18 objects through GUID32972.
    assert_eq!(
        digest(
            completed[278..296]
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "92171b0715a65ede4698e647b5c562f512862405f8e55fb170893aea0d47c27e"
    );
    // :60902340-:60940706 allocates 35 objects through GUID52348.
    assert_eq!(
        digest(births[439..474].iter().map(|&line| [line])),
        "d210a1745638804181c0abe45c8307d48d5727c45f4ca773b6f85242ede3db30"
    );
    // :60902332-:60940681 completes 35 objects through GUID33068.
    assert_eq!(
        digest(
            completed[296..331]
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "5908c0513cc4d1dab9534c0b6d56bc8abddb1e48aa092b224fb8e34bb8b48aff"
    );
    // :60943951-:61048471 allocates 97 further objects.
    assert_eq!(
        digest(births[474..].iter().map(|&line| [line])),
        "443a6da06194b5063326dae2237bc831e551a18cadae1cb5d753d2a8f650ba5e"
    );
    // :60943949-:61049110 completes 98 objects through GUID45604.
    assert_eq!(
        digest(
            completed[331..]
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "966ea90a26bedfbd99e1fda68a7ff2eaddcabe16e702b0b7328c5db601d72978"
    );
    let births = &births[223..265];
    let completed = &completed[116..157];
    // Trace :60554104-:60601783 and :60554098-:60601776.
    assert_eq!((births.len(), completed.len()), (42, 41));
    assert_eq!((births[0], births[41]), (60554104, 60601783));
    assert_eq!(
        (completed[0], completed[40]),
        ((60554098, 35891), (60601776, 35127))
    );
    assert_eq!(
        digest(births.iter().map(|&line| [line])),
        "be2fae9050a353058bd6d498414916acf3b6e5967ee514ea6ea1a6d9ed4743b0"
    );
    assert_eq!(
        digest(
            completed
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "0f080b6366ee991555f42ad70cd3dfee5bb0e2675a08ac3318772fae51524c42"
    );
}

#[derive(Default)]
pub(super) struct TransportSqlTrace {
    requests: Vec<(u64, Vec<u8>)>,
    clocks: Vec<u64>,
    result_births: Vec<u64>,
    result_deaths: Vec<u64>,
    result_rows: Vec<(u64, usize)>,
    period_rows: Vec<[u32; 2]>,
}
impl TransportSqlTrace {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str]) {
        match kind {
            "sql_request_bytes" => self.requests.push((line, unhex(p.last().unwrap()))),
            "clock_sample" => self.clocks.push(line),
            "logical_birth" => {
                assert_eq!(p[1], "sql_result");
                self.result_births.push(line);
            }
            "logical_death" => self.result_deaths.push(line),
            "sql_query_result" => self.result_rows.push((line, p[3].parse().unwrap())),
            "sql_field" => match p[1] {
                "0" => self.period_rows.push([sql_uint(p), 0]),
                "2" => self.period_rows.last_mut().unwrap()[1] = sql_uint(p),
                _ => (),
            },
            "rng_call" | "mt_raw" => panic!("unexpected transport template RNG at {line}"),
            _ => (),
        }
    }

    pub(super) fn assert_load(&self, actual: &[TransportTemplateEvent]) -> usize {
        let queries = actual.iter().filter_map(|event| match event {
            TransportTemplateEvent::SqlResult { statement, rows } => {
                Some((statement.as_bytes(), *rows))
            }
            _ => None,
        });
        assert_eq!(
            self.requests
                .iter()
                .map(|(line, _)| *line)
                .collect::<Vec<_>>(),
            [46233409, 46233692]
        );
        assert_eq!(self.result_rows, [(46233528, 9), (46233811, 0)]);
        assert_eq!(self.result_births, [46233527, 46233810]);
        assert_eq!(self.result_deaths, [46233678, 46233812]);
        assert_eq!(self.clocks, [46233411, 46233694]);
        assert!(queries.eq(self
            .requests
            .iter()
            .zip(&self.result_rows)
            .map(|((_, sql), (_, rows))| (sql.as_slice(), *rows))));
        assert!(actual
            .iter()
            .filter_map(|event| match event {
                TransportTemplateEvent::AssignedPeriod { entry, period, .. } =>
                    Some([*entry, *period]),
                _ => None,
            })
            .eq(self.period_rows.iter().copied()));
        self.requests.len()
    }
}

fn sql_uint(p: &[&str]) -> u32 {
    std::str::from_utf8(&unhex(p.last().unwrap()))
        .unwrap()
        .parse()
        .unwrap()
}

#[derive(Default)]
pub(super) struct TransportTrace {
    births: Vec<(u64, u64, &'static str)>,
    ids: Vec<(u64, [u64; 5])>,
    models: Vec<(u64, String)>,
    inserts: Vec<(u64, u64)>,
    returned: Option<u64>,
}
impl TransportTrace {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str]) {
        match kind {
            "logical_birth" => {
                let class = match p[1] {
                    "object" => "object",
                    "event_processor" => "event_processor",
                    "dynamic_model" => "dynamic_model",
                    other => panic!("unexpected transport birth: {other} at {line}"),
                };
                self.births.push((line, p[0].parse().unwrap(), class));
            }
            "object_identity" => self
                .ids
                .push((line, std::array::from_fn(|i| p[i + 1].parse().unwrap()))),
            "vmap_model_acquire" => self.models.push((line, p[1].to_owned())),
            "dynamic_insert" => self.inserts.push((line, p[1].parse().unwrap())),
            "thread_return" => {
                assert_eq!(p, ["7"]);
                assert!(self.returned.replace(line).is_none());
            }
            "object_construct"
            | "choice_queue_owner_binding"
            | "choice_queue_begin"
            | "choice_queue"
            | "choice_geometry_cache"
            | "dynamic_transform"
            | "dynamic_rotation"
            | "dynamic_bounds" => (),
            _ => panic!("unexpected map 1 transport record {kind} at {line}"),
        }
    }

    // These names also identify the live shared VMap acquisition reference counts.
    const MODELS: [&str; 2] = ["Transportship.wmo", "Transport_Zeppelin.wmo"];

    pub(super) fn model_references(vmap: &SourceVMapManager) -> [u32; 2] {
        let shared = vmap.collision.model_inventory();
        Self::MODELS.map(|name| {
            shared
                .iter()
                .find(|m| m.owner.name.as_ref() == name.as_bytes())
                .map_or(0, |m| m.references)
        })
    }

    pub(super) fn assert_live(
        &self,
        map: &SourceContinent,
        objects: &SourceObjects,
        models: &SourceDynamicModels,
        first_object: u64,
        first_model: u64,
    ) -> usize {
        assert_eq!(self.returned, Some(61049852)); // Map.cpp:228.
        let expected = [
            (
                20808,
                [61049124, 61049128, 61049130, 61049137, 61049140, 61049435],
                0,
                b"TEST Ship".as_slice(),
            ),
            (
                164871,
                [61049436, 61049438, 61049439, 61049440, 61049441, 61049745],
                1,
                b"Zeppelin - Orgrimmar to Undercity".as_slice(),
            ),
            (
                176244,
                [61049750, 61049754, 61049756, 61049759, 61049762, 61049793],
                0,
                b"Moonspray".as_slice(),
            ),
            (
                177233,
                [61049797, 61049801, 61049803, 61049806, 61049809, 61049844],
                0,
                b"Feathermoon Ferry".as_slice(),
            ),
        ];
        assert_eq!(
            (
                self.births.len(),
                self.ids.len(),
                self.models.len(),
                self.inserts.len()
            ),
            (12, 4, 4, 4)
        );
        assert_eq!(objects.construction_count(7) - first_object, 4);
        assert_eq!(models.models.next_generation() - first_model, 4);
        let inventory = models.models.inventory();
        assert_eq!(map.transports.members(map.identity).count(), 4);
        for (i, (entry, [birth, events, guid, model, acquire, insert], name, object_name)) in
            expected.into_iter().enumerate()
        {
            let object_id = 30064773281 + 3 * i as u64;
            assert_eq!(
                self.births[3 * i..3 * i + 3],
                [
                    (birth, object_id, "object"),
                    (events, object_id + 1, "event_processor"),
                    (model, object_id + 2, "dynamic_model")
                ]
            );
            assert_eq!(self.ids[i], (guid, [entry, entry, 0, 8128, object_id]));
            assert_eq!(
                (self.models[i].0, self.models[i].1.as_str()),
                (acquire, Self::MODELS[name])
            );
            assert_eq!(self.inserts[i], (insert, object_id + 2));

            let handle = ObjectHandle {
                thread: 7,
                construction: first_object + i as u64 + 1,
            };
            assert!(map.transports.contains(map.identity, handle));
            let object = objects.get(handle).unwrap();
            assert_eq!(object.kind, SourceObjectKind::Transport);
            assert_eq!(object.map, Some(map.identity));
            let identity = object.guid.unwrap();
            assert_eq!(
                (
                    object.database_guid,
                    identity.counter(),
                    identity.entry(),
                    identity.high()
                ),
                (entry as u32, entry as u32, 0, 8128)
            );
            assert_eq!(object.source_allocation, None);
            assert!(!object.in_world && !object.active);
            assert!(object.current_cell.is_none());
            assert_eq!(map.cells.registered_cell(handle), None);
            assert!(!map.cells.active().contains(handle));
            assert!(!objects.client_update_pending(map.identity, handle)); // Object.cpp:1107-1117.

            let transport = map.transports.get(handle).unwrap();
            assert!(transport.event_processor_is_initial());
            assert_eq!(transport.update_flags, 0x52); // Transports.cpp:120.
            assert_eq!(
                (
                    transport.entry,
                    transport.state,
                    transport.kind,
                    transport.animation
                ),
                (entry as u32, 1, 15, 100)
            );
            assert_eq!(transport.name.as_slice(), object_name);
            assert_eq!(transport.motion.progress(), 0); // World.cpp:107 and Map.cpp:78-80.
            assert!(transport.has_model);
            let actual_model = models.handle(handle).unwrap();
            assert_eq!(actual_model.pointer(), None);
            assert_eq!(actual_model.native_generation(), first_model + i as u64 + 1);
            assert!(map.dynamic_tree.tree.contains(actual_model));
            let actual = inventory
                .iter()
                .find(|m| m.handle == *actual_model)
                .unwrap();
            assert_eq!(actual.shared.name.as_ref(), Self::MODELS[name].as_bytes());
        }
        expected.len()
    }
}

pub(super) fn assert_after_garments(
    births: &[u64],
    completed: &[(u64, u32)],
    words: &[(u64, u64, u64)],
    clocks: &super::active_init::ActiveClocks,
) {
    // Map 0, :61479844 through the Elwynn guard factory at :61581173.
    assert_eq!(
        (births.len(), births[0], births[171]),
        (172, 61479846, 61577266)
    );
    assert_eq!(
        digest(births.iter().map(|&line| [line])),
        "baab52ca3cec8a95455ae598a4f642f32f8aa52d447fb07d19b4e175c6badfdf"
    );
    assert_eq!(
        (completed.len(), completed[0], completed[164]),
        (165, (61479844, 81085), (61577264, 80161))
    );
    assert_eq!(
        digest(
            completed
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "9df53bd39fabcf25a1f5c19f587241e76f5f40d5a699de744860bcb550d05f31"
    );
    assert_eq!(
        (words.len(), words[0], words[1044]),
        (
            1045,
            (61479864, 33206, 907412437),
            (61577601, 34250, 1316509942)
        )
    );
    assert_eq!(
        digest(words.iter().map(|&(line, raw, word)| [line, raw, word])),
        "56a128597c0fce20f0bacb7bccd45815f75f9e4c59fe1de2f1a3b31ff9996683"
    );
    assert_eq!(clocks.0[1064].time_line, 61577608);
    assert_eq!(
        digest(clocks.0.range(921..1065).map(|c| [
            c.site_line,
            c.time_line,
            c.thread,
            c.site[2],
            c.raw,
            c.seconds as u64
        ])),
        "940745b654fc9d5c6f144f13073265d3972a989ae85c09b4e862658c9a6e2d75"
    );
}

pub(super) fn assert_after_elwynn(
    births: &[u64],
    completed: &[(u64, u32)],
    words: &[(u64, u64, u64)],
    clocks: &super::active_init::ActiveClocks,
) {
    // Map 0, :61581298 through the last active preload at :61721572.
    assert_eq!(
        (births.len(), births[0], births[239]),
        (240, 61581300, 61720856)
    );
    assert_eq!(
        digest(births.iter().map(|&line| [line])),
        "fc4cd309b0de74d8d6a837e93960f8468928659c3376dc72fc4e2c7d21bc6b2f"
    );
    assert_eq!(
        (completed.len(), completed[0], completed[239]),
        (240, (61581298, 80263), (61721572, 92563))
    );
    assert_eq!(
        digest(
            completed
                .iter()
                .map(|&(line, guid)| [line, u64::from(guid)])
        ),
        "86880ee41fe401041f562140c384eead8ac04601051deaf6e9f8f2d3a9d9e368"
    );
    assert_eq!(
        (words.len(), words[0], words[1525]),
        (
            1526,
            (61581318, 34259, 3652573144),
            (61721552, 35784, 3678788077)
        )
    );
    assert_eq!(
        digest(words.iter().map(|&(line, raw, word)| [line, raw, word])),
        "4de8d9c16ffa8d80c68a2f5034691474c4d1ff1e071c1a3be5cb04c9691853f9"
    );
    assert_eq!(clocks.0[1283].time_line, 61721153);
    assert_eq!(
        digest(clocks.0.range(1065..1284).map(|c| [
            c.site_line,
            c.time_line,
            c.thread,
            c.site[2],
            c.raw,
            c.seconds as u64
        ])),
        "190a376d85a649157942c18256c8b670a9532024b12dbe17b5a308026ca6dfa6"
    );
}

fn digest<const N: usize>(rows: impl Iterator<Item = [u64; N]>) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for row in rows {
        for value in row {
            hash.update(value.to_le_bytes());
        }
    }
    format!("{:x}", hash.finalize())
}
