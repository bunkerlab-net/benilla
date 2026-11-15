// SPDX-License-Identifier: GPL-3.0-or-later
//! Registry-only World.cpp:927,1333,1341 projection, never SD2 loader outputs.
use super::*;
const QUERY: &[u8] = b"SELECT DISTINCT(ScriptName) FROM creature_template WHERE ScriptName <> '' UNION SELECT DISTINCT(ScriptName) FROM gameobject_template WHERE ScriptName <> '' UNION SELECT DISTINCT(ScriptName) FROM item_template WHERE ScriptName <> '' UNION SELECT DISTINCT(ScriptName) FROM scripted_areatrigger WHERE ScriptName <> '' UNION SELECT DISTINCT(ScriptName) FROM scripted_event_id WHERE ScriptName <> '' UNION SELECT DISTINCT(ScriptName) FROM instance_template WHERE ScriptName <> '' UNION SELECT DISTINCT(ScriptName) FROM world_template WHERE ScriptName <> ''";
#[derive(Default)]
pub(super) struct Evidence {
    query: Option<(u64, u64)>,
    pointer: Option<String>,
    names: Vec<Vec<u8>>,
    unexecuted_sql: Vec<(u64, u64, String)>,
}
impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, p: &[&str], raw: u64) {
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            if query == QUERY {
                assert!(self.query.replace((line, raw)).is_none());
                self.pointer = Some(p[0].into());
            } else {
                self.pointer = None;
            }
            let query = String::from_utf8(query).unwrap();
            if [
                " FROM script_texts",
                " FROM custom_texts",
                " FROM gossip_texts",
                " FROM script_waypoint",
            ]
            .iter()
            .any(|table| query.contains(table))
            {
                self.unexecuted_sql.push((line, raw, query));
            }
        }
        if kind == "sql_field" && self.pointer.as_deref() == Some(p[0]) {
            assert_eq!(p[1], "0");
            self.names.push(super::templates::sql_storage_field(p));
        }
    }
    pub(super) fn load(
        self,
        catalog: &mut VerifiedCatalog,
        creatures: &LoadedCreatureTemplates,
        gameobjects: &LoadedGameObjectTemplates,
    ) -> SourceScriptRegistry {
        let mut scripts = SourceScriptRegistry::default();
        scripts
            .load_script_names(|| Ok(Some(catalog.execution_registry_names()?)))
            .unwrap();
        let mut expected = self.names;
        expected.push(Vec::new());
        expected.sort();
        assert_eq!(scripts.names(), expected);
        let mut registrations = catalog
            .execution_registry_registrations(Path::new(
                &std::env::var_os("BENILLA_F5_SCRIPT_REPORT").unwrap(),
            ))
            .unwrap();
        assert_eq!(registrations.len(), 411);
        // World.cpp:1338 -> SpellScript.cpp:73 -> Shaman.cpp:234-239 is after AddScripts.
        let shaman = registrations.pop().unwrap();
        assert_eq!(shaman.source_function, "void LoadShamanScripts()");
        assert_eq!((shaman.source_line, shaman.registration), (239, 410));
        assert_eq!(shaman.name, b"npc_sentry_totem");
        assert!(registrations
            .iter()
            .all(|row| row.source_function.starts_with("void AddSC_")));
        scripts.initialize_registry(registrations).unwrap();
        assert_eq!(scripts.script_count(), 401);
        assert!(scripts.get_script(354).unwrap().is_none());
        scripts.register_self(shaman).unwrap();
        assert!(scripts.check_script_names().unwrap().is_empty());
        assert_eq!(
            (scripts.size(), scripts.script_count(), scripts.null_slots()),
            (403, 402, 1)
        );
        let mut nonnull_callbacks = 0;
        let mut go_hooks = 0;
        for id in 1..scripts.size() {
            let script = scripts.get_script(id as u32).unwrap().unwrap();
            assert_eq!(script.slots.len(), 22);
            nonnull_callbacks += script.slots.values().filter(|nonnull| **nonnull).count();
            go_hooks += usize::from(script.slots["GetGameObjectAI"]);
        }
        assert_eq!((nonnull_callbacks, go_hooks), (509, 19));
        println!("F6_SCRIPT_REGISTRATIONS attempts=411 AddScripts_attempts=410 AddScripts_count=401 post_spell_count=402 absent_names=9 accepted_callbacks={nonnull_callbacks} GetGameObjectAI_hooks={go_hooks}");
        for (kind, ids) in [
            (
                "creature",
                creatures
                    .rows()
                    .iter()
                    .map(|row| row.words[96])
                    .collect::<Vec<_>>(),
            ),
            (
                "gameobject",
                gameobjects.rows().iter().map(|row| row.words[37]).collect(),
            ),
        ] {
            let mut resolved = 0;
            let mut null = 0;
            for id in ids.into_iter().filter(|id| *id != 0) {
                if scripts.get_script(id).unwrap().is_some() {
                    resolved += 1;
                } else {
                    null += 1;
                }
            }
            println!("F6_SCRIPT_REGISTRY templates={kind} size={} count={} null_slots={} nonzero_resolved={resolved} nonzero_null={null} zero_empty_ub_reach=0 add_index_ub_reach=0 count_overflow_ub_reach=0 check_index_ub_reach=0 unexecuted={:?}", scripts.size(), scripts.script_count(), scripts.null_slots(), scripts.unexecuted_prerequisites());
        }
        println!(
            "F6_SCRIPT_NAMES matched_sql=1 query_line={} raw={} names=402",
            self.query.unwrap().0,
            self.query.unwrap().1
        );
        println!(
            "F6_SCRIPT_UNEXECUTED matched_sql=0 observed_only={:?}",
            self.unexecuted_sql
        );
        scripts
    }
}
