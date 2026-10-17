# Racial exploration starts

Normal P2P character creation uses the selected race's starting location without
additional configuration. Eight compact position facts cover all forty valid
Classic race/class pairs. Map, zone, XYZ and facing are persisted once, under
`ClassicRacialV1`; first login and every later entry use that saved pose.
Dwarves and gnomes share coordinates but have different initial facing.

Existing characters, including `NorthshireMeetingV1` characters, stay at their
saved positions. They are not automatically moved, even if never played. Create
a new character to use the new defaults. The legacy `ProfileStore::create_character`
API remains available for compatibility; the actual app creation selector uses
`create_character_at_racial_start` when no capture directory is configured.

## Source and scope

The original Rust lookup is
`crates/benilla-app/src/net/p2p/creation_start/defaults.rs`. Its eight starting-pose
facts were checked against [Classic-DB ec4f5961](https://github.com/cmangos/classic-db/tree/ec4f596146be6467ea93c57397858e329e2db852),
`Full_DB/ClassicDB_1_12_1_z2815.sql.gz`, SHA-256
`4f92db520868ab4e566726f68b5b2e380ae781209beaf22237b4f7f04600d0c0`.
All valid classes of a race share the same pose at this pin; no file under its
`Updates/` references `playercreateinfo`. Night Elf and Undead defaults at this
pin differ from core base SQL, so the database pin is the evidence for the facts.
The creation mechanism follows `Player::Create` and `ObjectMgr::LoadPlayerInfo`
in [CMaNGOS Classic 8ec338a](https://github.com/cmangos/mangos-classic/tree/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f).

This is a deliberately narrow factual position lookup, not a copy of SQL records,
a world database, a content pack, a native compiler or gameplay admission.
No client assets, text, models, maps, SQL, creatures, quests or item data are
included. The app code is GPL-3.0-or-later. Source AUTHORS, COPYRIGHT.md and
LICENSE.md are retained unchanged in the adjacent `creation_start/licenses/`
directory and packaged with the player. The source's copyright qualifications
remain applicable; attribution is not a blanket claim of legal clearance for
world-content redistribution. The [local-data boundary](CLASSIC-WORLD-IMPORT.md)
continues to prohibit bundled world content and SQL exports.

## Optional private overrides

`p2p.classic_start_captures` can override defaults for newly created characters
with the selected `playercreateinfo` pose in an existing private source-v2 capture.
It is optional; clearing the setting restores built-in defaults for future creation.

1. Follow the existing [read-only capture setup](../scripts/classic_export/README.md)
   with a v2 starter selection for each desired race/class. Keep captures private.
   This feature does not connect to SQL or invoke a capture subprocess.
2. Name files `<race>-<class>.json` in a private directory, for example `6-11.json`
   for Tauren Druid. Each file must select that exact race/class. Validate the full
   capture with `python3 scripts/classic-export.py validate <local-capture>`.
3. Set `classic_start_captures = "benilla-config/classic-starts"` under `[p2p]` in
   executable-adjacent `config.toml`, or edit the field in Connection setup and
   save/restart. Relative paths resolve beside the executable.
4. Create the character normally. Only the selected file is read. A missing,
   malformed, mismatched or unsupported capture refuses new creation without
   fallback, profile mutation or closing the lobby. Existing characters can still
   enter. Repairing the selected file can be retried without restarting; changing
   the configured directory requires a restart.

The bounded reader rejects special files and final symlinks/reparse points, reads
at most 64 MiB + one byte, and checks the selected row, finite canonical decimals,
checksum, race/class and source-v2 compatibility declarations. It interprets only
the position projection, not other world tables; full capture validation remains
separate. Revision declarations and digests are correlation, not proof of source
authenticity. The exact capture digest is stored in `ClassicSourceV1`.

## Compatibility and verification

The exploration content version is `benilla-saved-exploration-racial-starts-v5`.
All peers must update. Older builds cannot read the new spawn policy: keep a
private backup before testing and do not open updated profiles with older builds.
Different-continent peer rejection is unchanged. Capture paths/raw data never
cross the wire; peers receive saved character metadata and pose as before.

Headless tests compile the actual app creation selector and worldport projection.
They cover all forty valid pairs with no capture setting, first entry and reopen,
movement saves, archive restoration, legacy-pose preservation, explicit overrides,
and invalid/missing/mismatched capture refusal. Expected default poses are checked
against the pinned source. These are routing/persistence tests, not live terrain
or collision validation. Asset-backed Windows create/login/render testing still
requires the user's installation. NPCs, quests, spells, combat and progression are
outside this exploration change.
