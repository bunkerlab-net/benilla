// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw character storage and occurrence binding. No manager outcome is an input.
use super::*;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CharacterQueryStatus {
    Rows,
    Empty,
    Error,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterQueryOccurrence {
    pub owner: InputOwner,
    pub database_seconds: i64,
    pub world_seconds: i64,
    /// A complete source SQL occurrence permutation, not a GUID/key order.
    pub ordinals: Vec<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerExecutionBinding {
    pub format: String,
    pub execution_binding_sha256: Sha256Id,
    pub source_snapshot: String,
    pub initial_snapshot_sha256: Sha256Id,
    pub registry_sha256: Sha256Id,
    pub environment_sha256: Sha256Id,
    pub configuration: FileReceipt,
    pub creature: CharacterQueryOccurrence,
    pub gameobject: CharacterQueryOccurrence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitialRespawnRow {
    pub guid: u32,
    pub deadline: u64,
    pub instance: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitialInstanceRow {
    pub id: u32,
    pub map: u32,
    pub reset_time: u64,
    pub encounters: u32,
    pub data: Option<Vec<u8>>,
}
#[derive(Default)]
struct CharacterRelations {
    creatures: Vec<InitialRespawnRow>,
    gameobjects: Vec<InitialRespawnRow>,
    instances: BTreeMap<u32, InitialInstanceRow>,
}
/// Non-serializable, immutable and permanently non-admitting until F1/F7 integration.
pub struct ManagerInitialInputs<'a> {
    queries: [CharacterQueryOccurrence; 2],
    rows: CharacterRelations,
    held: Vec<ExecutionFile>,
    execution: Option<&'a VerifiedExecutionInputs>,
}
impl<'a> ManagerInitialInputs<'a> {
    pub fn open(
        inputs: &'a VerifiedExecutionInputs,
        path: &Path,
        approved_sha256: &Sha256Id,
    ) -> Result<Self> {
        let (held, binding) = manager_binding_file(path, approved_sha256)?;
        let configuration = binding
            .configuration
            .open(path.parent().unwrap_or_else(|| Path::new(".")), 1_048_576)?;
        Self::validate_identity(&binding, inputs.identity(), inputs.binding())?;
        let rows = CharacterRelations::read(inputs.manager_snapshot()?)?;
        Self::validate_queries(&binding.creature, &binding.gameobject, &rows)?;
        Ok(Self {
            queries: [binding.creature, binding.gameobject],
            rows,
            held: vec![held, configuration],
            execution: Some(inputs),
        })
    }
    fn validate_identity(
        binding: &ManagerExecutionBinding,
        identity: &Sha256Id,
        execution: &ExecutionBinding,
    ) -> Result<()> {
        require(
            binding.format == "benilla-source-manager-inputs-v1",
            "execution.managers.binding_format",
        )?;
        require(
            binding.execution_binding_sha256 == *identity,
            "execution.managers.execution_binding",
        )?;
        require(
            binding.source_snapshot == execution.source_snapshot
                && binding.initial_snapshot_sha256 == execution.initial_state.sha256,
            "execution.managers.snapshot_binding",
        )?;
        require(
            execution.preparation_input_bindings["registration"]["sha256"]
                == binding.registry_sha256.as_str()
                && execution.preparation_input_bindings["environment"]["sha256"]
                    == binding.environment_sha256.as_str(),
            "execution.managers.registry_environment_binding",
        )
    }
    fn validate_queries(
        creature: &CharacterQueryOccurrence,
        gameobject: &CharacterQueryOccurrence,
        rows: &CharacterRelations,
    ) -> Result<()> {
        for (query, rows, line, function) in [
            (
                creature,
                &rows.creatures,
                997,
                "MapPersistentStateManager::LoadCreatureRespawnTimes",
            ),
            (
                gameobject,
                &rows.gameobjects,
                1062,
                "MapPersistentStateManager::LoadGameobjectRespawnTimes",
            ),
        ] {
            query.owner.callsite.validate()?;
            require(
                query.owner.thread > 0
                    && query.owner.operation.thread == query.owner.thread
                    && query.owner.operation.ordinal > 0
                    && query.owner.occurrence > 0
                    && query.owner.map_instance.is_none()
                    && query.owner.object.is_none(),
                "execution.managers.query_owner",
            )?;
            require(
                query.owner.callsite.source_file == "src/game/Maps/MapPersistentStateMgr.cpp"
                    && query.owner.callsite.line == line
                    && query.owner.callsite.function == function,
                "execution.managers.query_callsite",
            )?;
            let surviving: BTreeSet<_> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| i128::from(row.deadline) > i128::from(query.database_seconds))
                .map(|(n, _)| n as u64)
                .collect();
            let supplied: BTreeSet<_> = query.ordinals.iter().copied().collect();
            require(
                supplied.len() == query.ordinals.len(),
                "execution.managers.duplicate_occurrence",
            )?;
            require(
                supplied.is_subset(&surviving),
                "execution.managers.query_occurrences",
            )?;
        }
        require(
            creature.owner.thread == gameobject.owner.thread
                && creature.owner.occurrence < gameobject.owner.occurrence,
            "execution.managers.query_order",
        )
    }
    /// Validates a result produced by the real query owner, never supplied by this binding.
    pub fn validate_query_result(
        &self,
        kind: GridSpawnKind,
        status: CharacterQueryStatus,
    ) -> Result<()> {
        self.check()?;
        let query = self.query(kind);
        let rows = match kind {
            GridSpawnKind::Creature => &self.rows.creatures,
            GridSpawnKind::GameObject => &self.rows.gameobjects,
        };
        let survivors = rows
            .iter()
            .filter(|row| i128::from(row.deadline) > i128::from(query.database_seconds))
            .count();
        match status {
            CharacterQueryStatus::Rows => require(
                !query.ordinals.is_empty() && query.ordinals.len() == survivors,
                "execution.managers.query_occurrences",
            ),
            CharacterQueryStatus::Empty => require(
                query.ordinals.is_empty() && survivors == 0,
                "execution.managers.false_empty_query",
            ),
            CharacterQueryStatus::Error => require(
                query.ordinals.is_empty(),
                "execution.managers.error_query_rows",
            ),
        }
    }
    pub fn check(&self) -> Result<()> {
        self.held.iter().try_for_each(ExecutionFile::check)?;
        if let Some(execution) = self.execution {
            execution.manager_snapshot()?;
        }
        Ok(())
    }
    pub fn require_sealed_execution(&self) -> Result<()> {
        Err(refuse("execution.f1_binding_pending"))
    }
    pub fn query(&self, kind: GridSpawnKind) -> &CharacterQueryOccurrence {
        match kind {
            GridSpawnKind::Creature => &self.queries[0],
            GridSpawnKind::GameObject => &self.queries[1],
        }
    }
    pub fn row(&self, kind: GridSpawnKind, ordinal: u64) -> Result<InitialRespawnRow> {
        self.check()?;
        let rows = match kind {
            GridSpawnKind::Creature => &self.rows.creatures,
            GridSpawnKind::GameObject => &self.rows.gameobjects,
        };
        rows.get(
            usize::try_from(ordinal).map_err(|_| refuse("execution.managers.query_occurrences"))?,
        )
        .copied()
        .ok_or_else(|| refuse("execution.managers.query_occurrences"))
    }
    pub fn instance(&self, id: u32) -> Option<&InitialInstanceRow> {
        self.rows.instances.get(&id)
    }
    /// MapManager.cpp:265-273, SELECT MAX(id) FROM instance. SQL NULL converts to zero.
    pub fn maximum_instance_id(&self) -> u32 {
        self.rows.instances.keys().next_back().copied().unwrap_or(0)
    }
    #[cfg(test)]
    pub(super) fn authored(raw: &[u8], binding: ManagerExecutionBinding) -> Result<Self> {
        let rows = CharacterRelations::parse(raw)?;
        Self::validate_queries(&binding.creature, &binding.gameobject, &rows)?;
        Ok(Self {
            queries: [binding.creature, binding.gameobject],
            rows,
            held: Vec::new(),
            execution: None,
        })
    }
    /// Frozen diagnostic inputs have no synthetic F1 identity or production admission.
    #[cfg(test)]
    pub(super) fn frozen_snapshot(
        raw: &[u8],
        mut queries: [CharacterQueryOccurrence; 2],
    ) -> Result<Self> {
        let rows = CharacterRelations::parse(raw)?;
        for (query, values) in queries.iter_mut().zip([&rows.creatures, &rows.gameobjects]) {
            query.ordinals = values
                .iter()
                .enumerate()
                .filter(|(_, row)| i128::from(row.deadline) > i128::from(query.database_seconds))
                .map(|(ordinal, _)| ordinal as u64)
                .collect();
        }
        Self::validate_queries(&queries[0], &queries[1], &rows)?;
        Ok(Self {
            queries,
            rows,
            held: Vec::new(),
            execution: None,
        })
    }
    #[cfg(test)]
    pub(super) fn frozen_query_result(&self, kind: GridSpawnKind) -> CharacterQueryStatus {
        let rows = match kind {
            GridSpawnKind::Creature => &self.rows.creatures,
            GridSpawnKind::GameObject => &self.rows.gameobjects,
        };
        if rows
            .iter()
            .any(|row| i128::from(row.deadline) > i128::from(self.query(kind).database_seconds))
        {
            CharacterQueryStatus::Rows
        } else {
            CharacterQueryStatus::Empty
        }
    }
}

fn manager_binding_file(
    path: &Path,
    approved_sha256: &Sha256Id,
) -> Result<(ExecutionFile, ManagerExecutionBinding)> {
    let (held, bytes) = metadata(path, 1_048_576)?;
    require(
        Sha256Id::of(&bytes) == *approved_sha256,
        "execution.managers.binding_anchor",
    )?;
    Ok((held, decode(&bytes, 1_048_576)?))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::native::source_execution::tests::{callsite, canonical, hash, Temp};

    pub(crate) fn snapshot(creatures: &str, gameobjects: &str, instances: &str) -> Vec<u8> {
        let mut text = String::new();
        for (table, data, columns) in [
            ("creature_respawn", creatures, COLUMNS[0]),
            ("gameobject_respawn", gameobjects, COLUMNS[1]),
            ("instance", instances, COLUMNS[2]),
        ] {
            text.push_str(&format!("CREATE TABLE `{table}` (\n"));
            for column in columns {
                text.push_str(&format!("  `{column}` bigint,\n"));
            }
            text.push_str(&format!(
                "  PRIMARY KEY (`{}`)\n) ENGINE=InnoDB;\nLOCK TABLES `{table}` WRITE;\n",
                columns[0]
            ));
            if !data.is_empty() {
                text.push_str(&format!("INSERT INTO `{table}` VALUES {data};\n"));
            }
            text.push_str("UNLOCK TABLES;\n");
        }
        text.into_bytes()
    }
    pub(crate) fn binding() -> ManagerExecutionBinding {
        let query = |occurrence, line, function: &str| {
            let mut callsite = callsite();
            callsite.source_file = "src/game/Maps/MapPersistentStateMgr.cpp".into();
            callsite.function = function.into();
            callsite.line = line;
            CharacterQueryOccurrence {
                owner: InputOwner {
                    thread: 1,
                    occurrence,
                    operation: OperationId {
                        thread: 1,
                        ordinal: occurrence,
                    },
                    map_instance: None,
                    object: None,
                    callsite,
                },
                database_seconds: 100,
                world_seconds: 100,
                ordinals: Vec::new(),
            }
        };
        ManagerExecutionBinding {
            format: "benilla-source-manager-inputs-v1".into(),
            execution_binding_sha256: hash(1),
            source_snapshot: "authored".into(),
            initial_snapshot_sha256: hash(2),
            registry_sha256: hash(3),
            environment_sha256: hash(4),
            configuration: FileReceipt {
                name: "configuration".into(),
                bytes: 0,
                sha256: hash(5),
            },
            creature: query(
                1,
                997,
                "MapPersistentStateManager::LoadCreatureRespawnTimes",
            ),
            gameobject: query(
                2,
                1062,
                "MapPersistentStateManager::LoadGameobjectRespawnTimes",
            ),
        }
    }
    #[test]
    fn maximum_instance_id_uses_the_bound_snapshot_and_empty_max_converts_to_zero() {
        for (rows, expected) in [("", 0), ("(3,0,0,0,''),(42,1,0,0,''),(7,0,0,0,'')", 42)] {
            let raw = snapshot("", "", rows);
            let inputs = ManagerInitialInputs::authored(&raw, binding()).unwrap();
            assert_eq!(inputs.maximum_instance_id(), expected);
        }
    }
    #[test]
    fn absent_character_relation_is_not_a_successful_empty_query() {
        let empty = snapshot("", "", "");
        let inputs = ManagerInitialInputs::authored(&empty, binding()).unwrap();
        inputs
            .validate_query_result(GridSpawnKind::Creature, CharacterQueryStatus::Empty)
            .unwrap();
        assert!(inputs.row(GridSpawnKind::Creature, 0).is_err());
        assert_eq!(
            inputs.require_sealed_execution().unwrap_err().id,
            "execution.f1_binding_pending"
        );
        for table in TABLES {
            let text = String::from_utf8(empty.clone()).unwrap();
            let start = text.find(&format!("CREATE TABLE `{table}` (")).unwrap();
            let end =
                start + text[start..].find("UNLOCK TABLES;\n").unwrap() + "UNLOCK TABLES;\n".len();
            let absent = format!("{}{}", &text[..start], &text[end..]);
            assert_eq!(
                ManagerInitialInputs::authored(absent.as_bytes(), binding())
                    .err()
                    .unwrap()
                    .id,
                "execution.managers.character_relation_absent"
            );
        }
    }
    #[test]
    fn database_cleanup_is_inclusive_and_independent_of_world_snapshot() {
        let raw = snapshot("(1,99,0),(2,100,0),(3,101,0),(4,102,0)", "", "");
        let mut bound = binding();
        bound.creature.ordinals = vec![3, 2];
        bound.creature.world_seconds = 101;
        let inputs = ManagerInitialInputs::authored(&raw, bound.clone()).unwrap();
        assert_eq!(inputs.query(GridSpawnKind::Creature).ordinals, [3, 2]);
        bound.creature.ordinals = vec![1, 2, 3];
        assert_eq!(
            ManagerInitialInputs::authored(&raw, bound)
                .err()
                .unwrap()
                .id,
            "execution.managers.query_occurrences"
        );
    }
    #[test]
    fn forged_duplicate_missing_or_out_of_range_occurrences_refuse() {
        let raw = snapshot("(1,200,0),(2,200,0)", "", "");
        for (ordinals, expected) in [
            (vec![0, 0], "execution.managers.duplicate_occurrence"),
            (vec![0], "execution.managers.query_occurrences"),
            (vec![0, 2], "execution.managers.query_occurrences"),
            (vec![], "execution.managers.query_occurrences"),
        ] {
            let mut bound = binding();
            bound.creature.ordinals = ordinals;
            assert_eq!(
                ManagerInitialInputs::authored(&raw, bound)
                    .and_then(|inputs| inputs
                        .validate_query_result(GridSpawnKind::Creature, CharacterQueryStatus::Rows))
                    .err()
                    .unwrap()
                    .id,
                expected
            );
        }
        assert_eq!(
            ManagerInitialInputs::authored(&raw, binding())
                .and_then(|inputs| inputs
                    .validate_query_result(GridSpawnKind::Creature, CharacterQueryStatus::Empty))
                .err()
                .unwrap()
                .id,
            "execution.managers.false_empty_query"
        );
    }
    #[test]
    fn query_status_is_owner_output_and_cannot_be_a_binding_input() {
        let raw = snapshot("(1,200,0)", "", "");
        let mut bound = binding();
        let inputs = ManagerInitialInputs::authored(&raw, bound.clone()).unwrap();
        inputs
            .validate_query_result(GridSpawnKind::Creature, CharacterQueryStatus::Error)
            .unwrap();
        assert_eq!(
            inputs
                .validate_query_result(GridSpawnKind::Creature, CharacterQueryStatus::Empty)
                .unwrap_err()
                .id,
            "execution.managers.false_empty_query"
        );
        bound.creature.ordinals.push(0);
        let inputs = ManagerInitialInputs::authored(&raw, bound).unwrap();
        inputs
            .validate_query_result(GridSpawnKind::Creature, CharacterQueryStatus::Rows)
            .unwrap();
        assert_eq!(
            inputs
                .validate_query_result(GridSpawnKind::Creature, CharacterQueryStatus::Error)
                .unwrap_err()
                .id,
            "execution.managers.error_query_rows"
        );
        let mut forged = serde_json::to_value(binding()).unwrap();
        forged["creature"]["status"] = serde_json::json!("error");
        assert!(decode::<ManagerExecutionBinding>(&canonical(&forged), 65536).is_err());
    }
    #[test]
    fn query_owner_callsite_and_source_order_cannot_be_forged() {
        for change in 0..5 {
            let mut bound = binding();
            let expected = match change {
                0 => {
                    bound.creature.owner.thread = 2;
                    "execution.managers.query_owner"
                }
                1 => {
                    bound.creature.owner.callsite.line = 998;
                    "execution.managers.query_callsite"
                }
                2 => {
                    bound.gameobject.owner.occurrence = 1;
                    "execution.managers.query_order"
                }
                3 => {
                    bound.creature.owner.callsite.core_revision = "other".into();
                    "execution.callsite"
                }
                _ => {
                    bound.creature.owner.map_instance = Some(MapIdentity {
                        map: 0,
                        instance: 0,
                        creation_generation: 1,
                    });
                    "execution.managers.query_owner"
                }
            };
            assert_eq!(
                ManagerInitialInputs::authored(&snapshot("", "", ""), bound)
                    .err()
                    .unwrap()
                    .id,
                expected
            );
        }
    }
    #[test]
    fn immutable_execution_snapshot_registry_and_environment_joins_refuse_forgery() {
        let bound = binding();
        let execution = ExecutionBinding {
            format: "benilla-source-execution-binding-v1".into(),
            catalog_sha256: hash(5),
            preparation_sha256: hash(6),
            source_snapshot: "authored".into(),
            schema_root_sha256: hash(7),
            preparation_input_bindings: serde_json::json!({"registration":{"sha256":hash(3)},"environment":{"sha256":hash(4)}}),
            reference_manifest_sha256: None,
            build_sha256: hash(8),
            schema_sha256: hash(9),
            instrumentation_sha256: hash(10),
            initial_state: FileReceipt {
                name: "initial".into(),
                bytes: 0,
                sha256: hash(2),
            },
            streams: Vec::new(),
            f1_binding: F1Binding::Pending {
                obligations: BTreeSet::new(),
            },
        };
        ManagerInitialInputs::validate_identity(&bound, &hash(1), &execution).unwrap();
        for change in 0..6 {
            let mut forged = bound.clone();
            let expected = match change {
                0 => {
                    forged.format = "other".into();
                    "execution.managers.binding_format"
                }
                1 => {
                    forged.execution_binding_sha256 = hash(11);
                    "execution.managers.execution_binding"
                }
                2 => {
                    forged.source_snapshot = "other".into();
                    "execution.managers.snapshot_binding"
                }
                3 => {
                    forged.initial_snapshot_sha256 = hash(11);
                    "execution.managers.snapshot_binding"
                }
                4 => {
                    forged.registry_sha256 = hash(11);
                    "execution.managers.registry_environment_binding"
                }
                _ => {
                    forged.environment_sha256 = hash(11);
                    "execution.managers.registry_environment_binding"
                }
            };
            assert_eq!(
                ManagerInitialInputs::validate_identity(&forged, &hash(1), &execution)
                    .unwrap_err()
                    .id,
                expected
            );
        }
    }
    #[test]
    fn manager_outcomes_and_sealed_status_are_not_input_types() {
        for family in [
            "pool_choice",
            "event_status",
            "spawn_result",
            "callback_result",
            "respawn_decision",
        ] {
            assert!(decode::<AdmissibleInput>(
                &canonical(&serde_json::json!({"family":family})),
                65536
            )
            .is_err());
        }
        for field in [
            "selected_pools",
            "effective_spawns",
            "deadline_answer",
            "sealed",
        ] {
            let mut raw = serde_json::to_value(binding()).unwrap();
            raw[field] = serde_json::json!(true);
            assert!(decode::<ManagerExecutionBinding>(&canonical(&raw), 65536).is_err());
        }
    }
    #[test]
    fn retained_snapshot_streaming_preserves_quoted_instance_data_and_rejects_changes() {
        let dir = Temp::new();
        let raw = snapshot(
            "(7,18446744073709551615,2)",
            "(7,300,2)",
            r"(2,33,500,3,'a\n\'b,()'),(3,34,600,0,NULL)",
        );
        let path = dir.0.join("snapshot");
        std::fs::write(&path, &raw).unwrap();
        let held = ExecutionFile::open(&path, 1_048_576).unwrap();
        let rows = CharacterRelations::read(&held).unwrap();
        assert_eq!(rows.creatures[0].deadline, u64::MAX);
        assert_eq!(rows.gameobjects[0].guid, 7);
        assert_eq!(
            rows.instances[&2].data.as_deref(),
            Some(b"a\n'b,()".as_slice())
        );
        assert_eq!(rows.instances[&3].data, None);
        #[cfg(unix)]
        {
            std::fs::write(&path, b"changed").unwrap();
            assert_eq!(
                CharacterRelations::read(&held).err().unwrap().id,
                "catalog.changed"
            );
        }
        #[cfg(windows)]
        {
            assert_eq!(
                std::fs::write(&path, b"changed")
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            assert_eq!(
                std::fs::rename(&path, dir.0.join("original"))
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            let unchanged = CharacterRelations::read(&held).unwrap();
            assert_eq!(unchanged.creatures, rows.creatures);
            assert_eq!(unchanged.gameobjects, rows.gameobjects);
            assert_eq!(unchanged.instances, rows.instances);
        }
    }
    #[test]
    fn malformed_snapshot_rows_schema_strings_and_duplicate_keys_refuse() {
        for (c, i, expected) in [
            (
                "(7,200,0),(7,300,0)",
                "",
                "execution.managers.snapshot_duplicate_key",
            ),
            (
                "(4294967296,200,0)",
                "",
                "execution.managers.snapshot_number",
            ),
            ("(7,NULL,0)", "", "execution.managers.snapshot_number"),
            ("(7,-1,0)", "", "execution.managers.snapshot_number"),
            ("(7,1,0),", "", "execution.managers.snapshot_insert"),
            (
                "",
                "(1,33,500,0,0x61)",
                "execution.managers.snapshot_string",
            ),
            (
                "",
                "(1,33,500,0,'unterminated)",
                "execution.managers.snapshot_string",
            ),
            (
                "",
                r"(1,33,500,0,'\q')",
                "execution.managers.snapshot_string",
            ),
        ] {
            assert_eq!(
                CharacterRelations::parse(&snapshot(c, "", i))
                    .err()
                    .unwrap()
                    .id,
                expected
            );
        }
        let raw = snapshot("", "", "");
        assert_eq!(
            CharacterRelations::parse(&raw[..raw.len() - 1])
                .err()
                .unwrap()
                .id,
            "execution.managers.snapshot_truncated"
        );
        let wrong_schema = String::from_utf8(raw)
            .unwrap()
            .replace("`respawntime`", "`wrong`");
        assert_eq!(
            CharacterRelations::parse(wrong_schema.as_bytes())
                .err()
                .unwrap()
                .id,
            "execution.managers.snapshot_schema"
        );
    }
    #[test]
    fn independent_manager_anchor_rejects_rehashed_forgery() {
        let dir = Temp::new();
        let path = dir.0.join("binding");
        let raw = canonical(&binding());
        let anchor = Sha256Id::of(&raw);
        std::fs::write(&path, &raw).unwrap();
        let (held, _) = manager_binding_file(&path, &anchor).unwrap();
        let mut forged = binding();
        forged.creature.database_seconds += 1;
        #[cfg(unix)]
        {
            std::fs::write(&path, canonical(&forged)).unwrap();
            assert_eq!(held.check().unwrap_err().id, "catalog.changed");
        }
        #[cfg(windows)]
        {
            assert_eq!(
                std::fs::write(&path, canonical(&forged))
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            assert_eq!(
                std::fs::rename(&path, dir.0.join("original"))
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            held.check().unwrap();
            assert_eq!(
                manager_binding_file(&path, &anchor)
                    .unwrap()
                    .1
                    .creature
                    .database_seconds,
                binding().creature.database_seconds
            );
        }
        drop(held);
        std::fs::write(&path, canonical(&forged)).unwrap();
        assert_eq!(
            manager_binding_file(&path, &anchor).err().unwrap().id,
            "execution.managers.binding_anchor"
        );
    }
    #[test]
    fn multiline_dump_rows_match_extended_insert_rows_and_truncation_refuses() {
        let raw = String::from_utf8(snapshot("(7,200,0),(8,300,0)", "", "")).unwrap();
        let multiline = raw.replace(
            "VALUES (7,200,0),(8,300,0);",
            "VALUES\n(7,200,0),\n(8,300,0);",
        );
        let rows = CharacterRelations::parse(multiline.as_bytes()).unwrap();
        assert_eq!(
            rows.creatures,
            [
                InitialRespawnRow {
                    guid: 7,
                    deadline: 200,
                    instance: 0
                },
                InitialRespawnRow {
                    guid: 8,
                    deadline: 300,
                    instance: 0
                }
            ]
        );
        let broken = multiline.replace("(8,300,0);", "(8,300,0)");
        assert_eq!(
            CharacterRelations::parse(broken.as_bytes())
                .err()
                .unwrap()
                .id,
            "execution.managers.snapshot_insert"
        );
        let outside = format!("INSERT INTO `creature_respawn` VALUES (7,200,0);\n{raw}");
        assert_eq!(
            CharacterRelations::parse(outside.as_bytes())
                .err()
                .unwrap()
                .id,
            "execution.managers.snapshot_insert_scope"
        );
        let shape = raw.replace(
            "INSERT INTO `creature_respawn` VALUES",
            "INSERT INTO `creature_respawn` (`guid`) VALUES",
        );
        assert_eq!(
            CharacterRelations::parse(shape.as_bytes())
                .err()
                .unwrap()
                .id,
            "execution.managers.snapshot_insert"
        );
        let duplicate = format!("{raw}{raw}");
        assert_eq!(
            CharacterRelations::parse(duplicate.as_bytes())
                .err()
                .unwrap()
                .id,
            "execution.managers.duplicate_relation"
        );
        let bad_lock = raw.replace(
            "CREATE TABLE `creature_respawn` (",
            "LOCK TABLES `creature_respawn` WRITE;\nCREATE TABLE `creature_respawn` (",
        );
        assert_eq!(
            CharacterRelations::parse(bad_lock.as_bytes())
                .err()
                .unwrap()
                .id,
            "execution.managers.snapshot_lock"
        );
        let statement = raw.replace("UNLOCK TABLES;", "UPDATE unexpected;\nUNLOCK TABLES;");
        assert_eq!(
            CharacterRelations::parse(statement.as_bytes())
                .err()
                .unwrap()
                .id,
            "execution.managers.snapshot_statement"
        );
    }
    #[test]
    fn oversized_snapshot_line_refuses_before_any_partial_relation_load() {
        let dir = Temp::new();
        let path = dir.0.join("oversized");
        let raw = vec![b'x'; 1_048_577];
        std::fs::write(&path, &raw).unwrap();
        let held = ExecutionFile::open(&path, 2_097_152).unwrap();
        assert_eq!(
            CharacterRelations::read(&held).err().unwrap().id,
            "execution.managers.snapshot_line_limit"
        );
    }
}

impl CharacterRelations {
    fn read(file: &ExecutionFile) -> Result<Self> {
        // Bounded streaming scan. No full dump clone and no external DB connection.
        let mut parser = SnapshotParser::default();
        let mut chunk = vec![0; 65_536];
        let mut line = Vec::new();
        let mut at = 0;
        while at < file.len() {
            let size = (file.len() - at).min(chunk.len() as u64) as usize;
            file.read_into(at, &mut chunk[..size])?;
            for &byte in &chunk[..size] {
                if byte == b'\n' {
                    parser.line(&line)?;
                    line.clear();
                } else {
                    require(
                        line.len() < 1_048_576,
                        "execution.managers.snapshot_line_limit",
                    )?;
                    line.push(byte);
                }
            }
            at += size as u64;
        }
        require(line.is_empty(), "execution.managers.snapshot_truncated")?;
        file.check()?;
        parser.finish()
    }
    #[cfg(test)]
    fn parse(raw: &[u8]) -> Result<Self> {
        require(
            raw.last() == Some(&b'\n'),
            "execution.managers.snapshot_truncated",
        )?;
        let mut parser = SnapshotParser::default();
        for line in raw[..raw.len() - 1].split(|b| *b == b'\n') {
            parser.line(line)?;
        }
        parser.finish()
    }
}
const TABLES: [&str; 3] = ["creature_respawn", "gameobject_respawn", "instance"];
const COLUMNS: [&[&str]; 3] = [
    &["guid", "respawntime", "instance"],
    &["guid", "respawntime", "instance"],
    &["id", "map", "resettime", "encountersMask", "data"],
];
const CREATE: [&[u8]; 3] = [
    b"CREATE TABLE `creature_respawn` (",
    b"CREATE TABLE `gameobject_respawn` (",
    b"CREATE TABLE `instance` (",
];
const LOCK: [&[u8]; 3] = [
    b"LOCK TABLES `creature_respawn` WRITE;",
    b"LOCK TABLES `gameobject_respawn` WRITE;",
    b"LOCK TABLES `instance` WRITE;",
];
const INSERT: [&[u8]; 3] = [
    b"INSERT INTO `creature_respawn` VALUES",
    b"INSERT INTO `gameobject_respawn` VALUES",
    b"INSERT INTO `instance` VALUES",
];
const INSERT_TABLE: [&[u8]; 3] = [
    b"INSERT INTO `creature_respawn`",
    b"INSERT INTO `gameobject_respawn`",
    b"INSERT INTO `instance`",
];
#[derive(Default)]
struct SnapshotParser {
    rows: CharacterRelations,
    created: [bool; 3],
    locked: [bool; 3],
    unlocked: [bool; 3],
    schema: Option<(usize, usize)>,
    current: Option<usize>,
    insert_table: Option<usize>,
    keys: BTreeSet<(usize, u32, u32)>,
}
impl SnapshotParser {
    fn line(&mut self, line: &[u8]) -> Result<()> {
        if let Some(table) = self.insert_table {
            let done = line.ends_with(b";");
            let body = line
                .strip_suffix(if done { b";" } else { b"," })
                .ok_or_else(|| refuse("execution.managers.snapshot_insert"))?;
            require(!body.is_empty(), "execution.managers.snapshot_insert")?;
            self.insert(table, body)?;
            if done {
                self.insert_table = None;
            }
            return Ok(());
        }
        if let Some((table, column)) = self.schema {
            if line.starts_with(b"  `") {
                let name = line[3..].split(|b| *b == b'`').next().unwrap();
                require(
                    COLUMNS[table]
                        .get(column)
                        .is_some_and(|c| c.as_bytes() == name),
                    "execution.managers.snapshot_schema",
                )?;
                self.schema = Some((table, column + 1));
            } else if line.starts_with(b") ENGINE=") {
                require(
                    column == COLUMNS[table].len(),
                    "execution.managers.snapshot_schema",
                )?;
                self.schema = None;
            } else {
                require(
                    line.starts_with(b"  PRIMARY KEY") || line.starts_with(b"  KEY"),
                    "execution.managers.snapshot_schema",
                )?;
            }
            return Ok(());
        }
        for table in 0..TABLES.len() {
            if line == CREATE[table] {
                require(
                    !self.created[table],
                    "execution.managers.duplicate_relation",
                )?;
                self.created[table] = true;
                self.schema = Some((table, 0));
                return Ok(());
            }
            if line == LOCK[table] {
                require(
                    self.created[table] && !self.locked[table] && self.current.is_none(),
                    "execution.managers.snapshot_lock",
                )?;
                self.locked[table] = true;
                self.current = Some(table);
                return Ok(());
            }
            if let Some(rest) = line.strip_prefix(INSERT[table]) {
                require(
                    self.current == Some(table),
                    "execution.managers.snapshot_insert_scope",
                )?;
                if rest.is_empty() {
                    self.insert_table = Some(table);
                    return Ok(());
                }
                let body = rest
                    .strip_prefix(b" ")
                    .and_then(|body| body.strip_suffix(b";"))
                    .filter(|body| !body.is_empty())
                    .ok_or_else(|| refuse("execution.managers.snapshot_insert"))?;
                return self.insert(table, body);
            }
            // A different INSERT shape cannot silently turn a populated relation into empty.
            require(
                !line.starts_with(INSERT_TABLE[table]),
                "execution.managers.snapshot_insert",
            )?;
        }
        if line == b"UNLOCK TABLES;" {
            if let Some(table) = self.current.take() {
                self.unlocked[table] = true;
            }
        } else if self.current.is_some() {
            require(
                line.starts_with(b"/*!40000 ALTER TABLE `")
                    && (line.ends_with(b" DISABLE KEYS */;")
                        || line.ends_with(b" ENABLE KEYS */;")),
                "execution.managers.snapshot_statement",
            )?;
        }
        Ok(())
    }
    fn insert(&mut self, table: usize, mut bytes: &[u8]) -> Result<()> {
        while !bytes.is_empty() {
            require(
                bytes.first() == Some(&b'('),
                "execution.managers.snapshot_insert",
            )?;
            bytes = &bytes[1..];
            let mut values: [Option<Cow<'_, [u8]>>; 5] = std::array::from_fn(|_| None);
            for (column, slot) in values.iter_mut().enumerate().take(COLUMNS[table].len()) {
                let (value, rest) = sql_value(bytes)?;
                *slot = value;
                let delimiter = if column + 1 == COLUMNS[table].len() {
                    b')'
                } else {
                    b','
                };
                require(
                    rest.first() == Some(&delimiter),
                    "execution.managers.snapshot_insert",
                )?;
                bytes = &rest[1..];
            }
            let number = |n: usize| -> Result<u64> {
                let value = values[n]
                    .as_deref()
                    .ok_or_else(|| refuse("execution.managers.snapshot_number"))?;
                require(
                    !value.is_empty() && value.iter().all(u8::is_ascii_digit),
                    "execution.managers.snapshot_number",
                )?;
                std::str::from_utf8(value)
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| refuse("execution.managers.snapshot_number"))
            };
            let narrow = |n| -> Result<u32> {
                u32::try_from(number(n)?).map_err(|_| refuse("execution.managers.snapshot_number"))
            };
            if table < 2 {
                let row = InitialRespawnRow {
                    guid: narrow(0)?,
                    deadline: number(1)?,
                    instance: narrow(2)?,
                };
                require(
                    self.keys.insert((table, row.guid, row.instance)),
                    "execution.managers.snapshot_duplicate_key",
                )?;
                let rows = if table == 0 {
                    &mut self.rows.creatures
                } else {
                    &mut self.rows.gameobjects
                };
                require(
                    rows.len() < 2_000_000,
                    "execution.managers.snapshot_row_limit",
                )?;
                rows.push(row);
            } else {
                require(
                    matches!(values[4], None | Some(Cow::Owned(_))),
                    "execution.managers.snapshot_string",
                )?;
                let row = InitialInstanceRow {
                    id: narrow(0)?,
                    map: narrow(1)?,
                    reset_time: number(2)?,
                    encounters: narrow(3)?,
                    data: values[4].take().map(Cow::into_owned),
                };
                require(
                    self.rows.instances.len() < 2_000_000,
                    "execution.managers.snapshot_row_limit",
                )?;
                require(
                    self.rows.instances.insert(row.id, row).is_none(),
                    "execution.managers.snapshot_duplicate_key",
                )?;
            }
            if bytes.is_empty() {
                break;
            }
            require(
                bytes.first() == Some(&b','),
                "execution.managers.snapshot_insert",
            )?;
            bytes = &bytes[1..];
            require(!bytes.is_empty(), "execution.managers.snapshot_insert")?;
        }
        Ok(())
    }
    fn finish(self) -> Result<CharacterRelations> {
        require(
            self.created.iter().all(|v| *v)
                && self.locked.iter().all(|v| *v)
                && self.unlocked.iter().all(|v| *v)
                && self.schema.is_none()
                && self.current.is_none()
                && self.insert_table.is_none(),
            "execution.managers.character_relation_absent",
        )?;
        Ok(self.rows)
    }
}
type SqlValue<'a> = (Option<Cow<'a, [u8]>>, &'a [u8]);
pub(super) fn sql_value(bytes: &[u8]) -> Result<SqlValue<'_>> {
    if bytes.first() == Some(&b'\'') {
        let mut value = Vec::new();
        let mut at = 1;
        while at < bytes.len() {
            match bytes[at] {
                b'\'' => return Ok((Some(Cow::Owned(value)), &bytes[at + 1..])),
                b'\\' => {
                    at += 1;
                    let byte = *bytes
                        .get(at)
                        .ok_or_else(|| refuse("execution.managers.snapshot_string"))?;
                    value.push(match byte {
                        b'0' => 0,
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'b' => 8,
                        b'Z' => 26,
                        b'\\' | b'\'' | b'"' => byte,
                        _ => return Err(refuse("execution.managers.snapshot_string")),
                    });
                }
                byte => value.push(byte),
            }
            at += 1;
        }
        Err(refuse("execution.managers.snapshot_string"))
    } else {
        let end = bytes
            .iter()
            .position(|b| matches!(*b, b',' | b')'))
            .ok_or_else(|| refuse("execution.managers.snapshot_insert"))?;
        let value = &bytes[..end];
        Ok((
            if value == b"NULL" {
                None
            } else {
                Some(Cow::Borrowed(value))
            },
            &bytes[end..],
        ))
    }
}
