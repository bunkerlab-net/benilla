// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::VerifiedCatalog;
use std::collections::{BTreeMap, BTreeSet};

pub const F2_CATALOG_SHA256: &str =
    "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6";
pub const F2_PREPARATION_SHA256: &str =
    "7132d11a566eaf5cdccddd02a6e77746b46769fe54ac2d2e44fd27065572654e";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingF1Obligation {
    SealedReferenceExecution,
    InitialSnapshotJoin,
    SqlOccurrenceCausality,
    RegistryOccurrences,
    CompleteCausalCoverage,
    ObserverNonperturbation,
    UnsupportedSemanticOwners,
}
const REQUIRED_PENDING: [PendingF1Obligation; 7] = [
    PendingF1Obligation::SealedReferenceExecution,
    PendingF1Obligation::InitialSnapshotJoin,
    PendingF1Obligation::SqlOccurrenceCausality,
    PendingF1Obligation::RegistryOccurrences,
    PendingF1Obligation::CompleteCausalCoverage,
    PendingF1Obligation::ObserverNonperturbation,
    PendingF1Obligation::UnsupportedSemanticOwners,
];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "status", rename_all = "snake_case")]
pub enum F1Binding {
    Pending {
        obligations: BTreeSet<PendingF1Obligation>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileReceipt {
    pub name: String,
    pub bytes: u64,
    pub sha256: Sha256Id,
}
impl FileReceipt {
    pub(super) fn open(&self, root: &Path, limit: u64) -> Result<ExecutionFile> {
        require(self.bytes <= limit, "execution.file_limit")?;
        let held = ExecutionFile::open(&relative(root, &self.name)?, limit)?;
        require(held.len() == self.bytes, "execution.file_length")?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0; (self.bytes as usize).min(1_048_576)];
        let mut at = 0;
        while at < self.bytes {
            let size = (self.bytes - at).min(buffer.len() as u64) as usize;
            held.read_into(at, &mut buffer[..size])?;
            hash.update(&buffer[..size]);
            at += size as u64;
        }
        require(
            format!("{:x}", hash.finalize()) == self.sha256.0,
            "execution.file_hash",
        )?;
        held.check()?;
        Ok(held)
    }
}

/// The approved hash is supplied independently, not recovered from this document.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionBinding {
    pub format: String,
    pub catalog_sha256: Sha256Id,
    pub preparation_sha256: Sha256Id,
    pub source_snapshot: String,
    pub schema_root_sha256: Sha256Id,
    pub preparation_input_bindings: serde_json::Value,
    #[serde(deserialize_with = "required_option")]
    pub reference_manifest_sha256: Option<Sha256Id>,
    pub build_sha256: Sha256Id,
    pub schema_sha256: Sha256Id,
    pub instrumentation_sha256: Sha256Id,
    pub initial_state: FileReceipt,
    pub streams: Vec<InputStream>,
    pub f1_binding: F1Binding,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputStream {
    pub receipt: FileReceipt,
    pub records: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputOwner {
    pub thread: u64,
    pub occurrence: u64,
    pub operation: OperationId,
    #[serde(deserialize_with = "required_option")]
    pub map_instance: Option<MapIdentity>,
    #[serde(deserialize_with = "required_option")]
    pub object: Option<ObjectIdentity>,
    pub callsite: Callsite,
}

// Raw words and semantic results are absent here; Util.cpp:29-39 supplies only MT initialization.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "family", rename_all = "snake_case")]
pub enum AdmissibleInput {
    ClockObservation {
        clock: String,
        value: TypedBytes,
    },
    MtInitialState {
        engine: u64,
        words: Vec<u32>,
        index: u16,
    },
    ExternalSessionPacket {
        session: u64,
        opcode: u16,
        bytes: Vec<u8>,
    },
}
impl AdmissibleInput {
    fn validate(&self) -> Result<()> {
        match self {
            Self::ClockObservation { clock, value } => {
                require(!clock.is_empty(), "execution.clock_owner")?;
                value.validate()?;
                require(
                    matches!(
                        value.ty,
                        ScalarType::U32 | ScalarType::U64 | ScalarType::I64
                    ),
                    "execution.clock_width",
                )
            }
            Self::MtInitialState {
                engine,
                words,
                index,
            } => require(
                *engine > 0 && words.len() == 624 && *index <= 624 && words.iter().any(|w| *w != 0),
                "execution.mt_state",
            ),
            Self::ExternalSessionPacket { session, bytes, .. } => require(
                *session > 0 && bytes.len() <= 32_768,
                "execution.session_input",
            ),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputRecord {
    owner: InputOwner,
    value: AdmissibleInput,
}

struct Tape {
    descriptor: InputStream,
    held: ExecutionFile,
    offset: u64,
    cursor: u64,
}
impl Tape {
    fn next(&mut self, owner: &InputOwner) -> Result<AdmissibleInput> {
        require(
            self.cursor < self.descriptor.records,
            "execution.input_exhausted",
        )?;
        let mut length = [0; 4];
        self.held.read_into(self.offset, &mut length)?;
        let n = u32::from_le_bytes(length) as usize;
        require(n > 0 && n <= 65_536, "execution.input_record_limit")?;
        let mut raw = vec![0; n];
        self.held.read_into(self.offset + 4, &mut raw)?;
        let record: InputRecord = decode(&raw, 65_536)?;
        require(record.owner == *owner, "execution.input_identity")?;
        require(
            owner.thread > 0
                && owner.thread == owner.operation.thread
                && owner.occurrence == self.cursor + 1,
            "execution.input_occurrence",
        )?;
        record.owner.callsite.validate()?;
        record.value.validate()?;
        self.offset += 4 + n as u64;
        self.cursor += 1;
        Ok(record.value)
    }
    fn finish(&self) -> Result<()> {
        self.held.check()?;
        require(
            self.cursor == self.descriptor.records && self.offset == self.held.len(),
            "execution.input_surplus",
        )
    }
}

/// This handle owns only admissible inputs. Expected trace records live in TraceReader.
/// It is deliberately neither serializable nor a NativeCapability.
pub struct VerifiedExecutionInputs {
    catalog: VerifiedCatalog,
    binding: ExecutionBinding,
    identity: Sha256Id,
    held_metadata: Vec<ExecutionFile>,
    initial_state: ExecutionFile,
    tapes: BTreeMap<String, Tape>,
    failed: bool,
}
impl VerifiedExecutionInputs {
    pub fn open(
        catalog: VerifiedCatalog,
        preparation: &Path,
        binding_path: &Path,
        approved_binding_sha256: &Sha256Id,
        input_root: &Path,
    ) -> Result<Self> {
        let (binding_file, binding) = anchored_binding(binding_path, approved_binding_sha256)?;
        let (preparation_file, raw) = metadata(preparation, 1_048_576)?;
        let preparation_value: serde_json::Value = decode(&raw, 1_048_576)?;
        validate_binding(&binding, &preparation_value, catalog.execution_identity())?;
        let initial_state = binding.initial_state.open(input_root, 1_073_741_824)?;
        let mut tapes = BTreeMap::new();
        require(binding.streams.len() <= 64, "execution.stream_limit")?;
        for stream in &binding.streams {
            let held = stream.receipt.open(input_root, 1_073_741_824)?;
            require(
                stream.records <= stream.receipt.bytes / 5,
                "execution.input_count",
            )?;
            let tape = Tape {
                descriptor: stream.clone(),
                held,
                offset: 0,
                cursor: 0,
            };
            require(
                tapes.insert(stream.receipt.name.clone(), tape).is_none(),
                "execution.duplicate_stream",
            )?;
        }
        Ok(Self {
            catalog,
            binding,
            identity: approved_binding_sha256.clone(),
            held_metadata: vec![binding_file, preparation_file],
            initial_state,
            tapes,
            failed: false,
        })
    }
    pub fn binding(&self) -> &ExecutionBinding {
        &self.binding
    }
    pub fn identity(&self) -> &Sha256Id {
        &self.identity
    }
    pub fn verify_prefix(&self, prefix: &PrefixDescriptor) -> Result<()> {
        require(
            self.binding.reference_manifest_sha256.is_some(),
            "execution.f1_binding_pending",
        )?;
        require(
            prefix.binding_sha256 == self.identity
                && self.binding.reference_manifest_sha256.as_ref()
                    == Some(&prefix.reference_manifest_sha256)
                && prefix.schema_sha256 == self.binding.schema_sha256
                && prefix.instrumentation_sha256 == self.binding.instrumentation_sha256,
            "execution.prefix_binding",
        )
    }
    pub fn pending(&self) -> &BTreeSet<PendingF1Obligation> {
        let F1Binding::Pending { obligations } = &self.binding.f1_binding;
        obligations
    }
    pub fn require_whole_prefix(&self) -> Result<()> {
        Err(refuse("execution.f1_binding_pending"))
    }
    pub fn cursors(&self) -> BTreeMap<String, u64> {
        self.tapes
            .iter()
            .map(|(name, tape)| (name.clone(), tape.cursor))
            .collect()
    }
    pub fn consume(&mut self, stream: &str, owner: &InputOwner) -> Result<AdmissibleInput> {
        require(!self.failed, "execution.stopped")?;
        let result = self.check_handles().and_then(|()| {
            self.tapes
                .get_mut(stream)
                .ok_or_else(|| refuse("execution.unsupported_input_owner"))?
                .next(owner)
        });
        self.failed |= result.is_err();
        result
    }
    pub fn finish(&mut self) -> Result<()> {
        require(!self.failed, "execution.stopped")?;
        let result = self
            .check_handles()
            .and_then(|()| self.tapes.values().try_for_each(Tape::finish));
        self.failed |= result.is_err();
        result
    }
    fn check_handles(&self) -> Result<()> {
        for file in &self.held_metadata {
            file.check()?;
        }
        self.initial_state.check()?;
        Ok(())
    }
    /// Joins raw cells to a retained F2 occurrence, never a tape-authored query answer.
    pub fn compare_catalog_occurrence(
        &mut self,
        table: &str,
        ordinal: u64,
        raw_cells: &[Option<&[u8]>],
    ) -> Result<()> {
        require(!self.failed, "execution.stopped")?;
        let result = self
            .catalog
            .execution_row(table, ordinal, |row| {
                let cells: Vec<_> = row.cells().collect::<std::result::Result<_, _>>()?;
                if row.ordinal != ordinal || cells != raw_cells {
                    return Err(super::super::production_compiler::ProductionDiagnostic {
                        id: "execution.catalog_occurrence".into(),
                    });
                }
                Ok(())
            })
            .map_err(Refusal::from);
        self.failed |= result.is_err();
        result
    }
}

fn validate_binding(
    binding: &ExecutionBinding,
    preparation: &serde_json::Value,
    catalog: &serde_json::Value,
) -> Result<()> {
    require(
        binding.format == "benilla-source-execution-binding-v1",
        "execution.binding_format",
    )?;
    require(
        binding.catalog_sha256.as_str() == F2_CATALOG_SHA256
            && catalog["catalog_sha256"] == F2_CATALOG_SHA256
            && preparation["catalog_sha256"] == F2_CATALOG_SHA256,
        "execution.catalog_identity",
    )?;
    let mut unsigned = preparation.clone();
    let claimed = unsigned
        .as_object_mut()
        .ok_or_else(|| refuse("execution.preparation"))?
        .remove("preparation_sha256");
    let mut hash = Sha256::new();
    hash.update(b"benilla-native-catalog-preparation-v2\0");
    hash.update(serde_json::to_vec(&unsigned).map_err(|_| refuse("execution.preparation"))?);
    require(
        claimed == Some(serde_json::json!(F2_PREPARATION_SHA256))
            && binding.preparation_sha256.as_str() == F2_PREPARATION_SHA256
            && format!("{:x}", hash.finalize()) == F2_PREPARATION_SHA256,
        "execution.preparation_identity",
    )?;
    require(
        preparation["format"] == "benilla-native-catalog-preparation-v2"
            && preparation["complete"] == false,
        "execution.preparation_status",
    )?;
    require(
        catalog["source_snapshot"] == binding.source_snapshot
            && preparation["source_snapshot"] == binding.source_snapshot
            && catalog["schema_root"]["sha256"] == binding.schema_root_sha256.as_str(),
        "execution.snapshot_binding",
    )?;
    require(
        preparation["input_bindings"] == binding.preparation_input_bindings,
        "execution.occurrence_binding",
    )?;
    require(
        binding.preparation_input_bindings["initial_snapshot"]["sha256"]
            == binding.initial_state.sha256.as_str()
            && binding.preparation_input_bindings["initial_snapshot"]["bytes"]
                == binding.initial_state.bytes,
        "execution.initial_snapshot_binding",
    )?;
    let F1Binding::Pending { obligations } = &binding.f1_binding;
    require(
        REQUIRED_PENDING.iter().all(|p| obligations.contains(p)),
        "execution.pending_obligations",
    )
}

fn anchored_binding(path: &Path, anchor: &Sha256Id) -> Result<(ExecutionFile, ExecutionBinding)> {
    let (held, raw) = metadata(path, 1_048_576)?;
    require(Sha256Id::of(&raw) == *anchor, "execution.binding_anchor")?;
    Ok((held, decode(&raw, 1_048_576)?))
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
