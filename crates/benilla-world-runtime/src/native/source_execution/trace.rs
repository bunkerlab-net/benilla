// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventKey {
    pub thread: u64,
    pub sequence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationId {
    pub thread: u64,
    pub ordinal: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapIdentity {
    pub map: u32,
    pub instance: u32,
    pub creation_generation: u64,
}

/// Component operations, not fabricated F0 envelopes or admissible inputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum PersistentOperation {
    Created {
        state: PersistentStateIdentity,
    },
    Binding {
        state: PersistentStateIdentity,
        before: Option<MapIdentity>,
        after: Option<MapIdentity>,
    },
    Deadline {
        state: PersistentStateIdentity,
        kind: GridSpawnKind,
        guid: u32,
        requested: i64,
        world_seconds: i64,
        before: i64,
        after: i64,
    },
    Overlay {
        state: PersistentStateIdentity,
        cell_key: u32,
        kind: GridSpawnKind,
        guid: u32,
        insert: bool,
        changed: bool,
    },
    Unloaded {
        state: PersistentStateIdentity,
    },
    DungeonBind {
        state: PersistentStateIdentity,
        kind: PersistentBindKind,
        owner: u64,
        add: bool,
        before: usize,
        after: usize,
    },
    InitialQuery {
        kind: GridSpawnKind,
        status: CharacterQueryStatus,
        database_seconds: i64,
        world_seconds: i64,
        loaded: u64,
    },
    InitialRow {
        kind: GridSpawnKind,
        ordinal: u64,
        disposition: RespawnDisposition,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectIdentity {
    pub object_type: String,
    #[serde(deserialize_with = "required_option")]
    pub source_guid: Option<u64>,
    #[serde(deserialize_with = "required_option")]
    pub runtime_guid: Option<u64>,
    pub construction_generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Callsite {
    pub core_revision: String,
    pub source_file: String,
    pub line: u32,
    pub function: String,
    pub inlined_caller_chain: Vec<String>,
    pub site_ordinal: u64,
}
impl Callsite {
    pub(super) fn validate(&self) -> Result<()> {
        require(
            self.core_revision == "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
                && self.line > 0
                && !self.source_file.is_empty()
                && !self.function.is_empty()
                && self.site_ordinal > 0,
            "execution.callsite",
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalarType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    Binary32,
    Binary64,
    Bytes,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedBytes {
    pub ty: ScalarType,
    pub bytes: Vec<u8>,
}
impl TypedBytes {
    pub(super) fn validate(&self) -> Result<()> {
        let width = match self.ty {
            ScalarType::U8 | ScalarType::I8 => 1,
            ScalarType::U16 | ScalarType::I16 => 2,
            ScalarType::U32 | ScalarType::I32 | ScalarType::Binary32 => 4,
            ScalarType::U64 | ScalarType::I64 | ScalarType::Binary64 => 8,
            ScalarType::Bytes => return Ok(()),
        };
        require(self.bytes.len() == width, "execution.payload_width")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    OperationBegin,
    OperationEnd,
    Process,
    Thread,
    ConfigRead,
    ClockRead,
    WorldUpdate,
    MapUpdate,
    GridLifecycle,
    CellLifecycle,
    ConstructionRemoval,
    Container,
    Visitor,
    UpdateSet,
    RngInit,
    RngRaw,
    RngSemantic,
    Callback,
    Geometry,
    SqlRequest,
    SqlRows,
    SqlWrite,
    Async,
    Message,
    Network,
    Manager,
    State,
    Checkpoint,
}
/// All F0 envelope fields are required, including explicit null context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceRecord {
    pub id: u64,
    pub kind: EventKind,
    pub thread: u64,
    pub thread_sequence: u64,
    #[serde(deserialize_with = "required_option")]
    pub operation: Option<OperationId>,
    #[serde(deserialize_with = "required_option")]
    pub parent_operation: Option<OperationId>,
    pub cause_events: Vec<u64>,
    #[serde(deserialize_with = "required_option")]
    pub world_update: Option<u64>,
    #[serde(deserialize_with = "required_option")]
    pub map_update: Option<u64>,
    #[serde(deserialize_with = "required_option")]
    pub map_instance: Option<MapIdentity>,
    #[serde(deserialize_with = "required_option")]
    pub object: Option<ObjectIdentity>,
    pub callsite: Callsite,
    pub payload: BTreeMap<String, TypedBytes>,
}
impl TraceRecord {
    pub fn key(&self) -> EventKey {
        EventKey {
            thread: self.thread,
            sequence: self.thread_sequence,
        }
    }
    fn validate(&self) -> Result<()> {
        require(
            self.id > 0 && self.thread > 0 && self.thread_sequence > 0,
            "execution.event_identity",
        )?;
        self.callsite.validate()?;
        require(
            self.operation
                .is_none_or(|o| o.thread == self.thread && o.ordinal > 0),
            "execution.operation_owner",
        )?;
        require(
            self.map_instance.is_none_or(|m| m.creation_generation > 0)
                && (self.map_update.is_none() || self.map_instance.is_some()),
            "execution.map_generation",
        )?;
        require(
            self.object
                .as_ref()
                .is_none_or(|o| o.construction_generation > 0 && !o.object_type.is_empty()),
            "execution.object_generation",
        )?;
        for value in self.payload.values() {
            value.validate()?;
        }
        require(
            self.cause_events
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                == self.cause_events.len(),
            "execution.duplicate_cause",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundary {
    pub name: String,
    pub per_thread: BTreeMap<u64, u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    F0StructuralV1,
    OriginalUnsealedRngV1,
    OriginalUnsealedSchedulerV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefixDescriptor {
    pub binding_sha256: Sha256Id,
    pub reference_manifest_sha256: Sha256Id,
    pub schema_sha256: Sha256Id,
    pub instrumentation_sha256: Sha256Id,
    pub projection: Projection,
    pub start: Boundary,
    pub end: Boundary,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceHeader {
    pub format: String,
    pub reference_manifest_sha256: Sha256Id,
    pub schema_sha256: Sha256Id,
    pub instrumentation_sha256: Sha256Id,
    pub start_boundary: Boundary,
    pub coverage: BTreeSet<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceChunk {
    pub index: u64,
    pub first_event: u64,
    pub last_event: u64,
    pub bytes: u64,
    pub sha256: Sha256Id,
    #[serde(deserialize_with = "required_option")]
    pub previous_chunk_sha256: Option<Sha256Id>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceFooter {
    pub end_boundary: Boundary,
    pub event_count: u64,
    pub per_thread_counts: BTreeMap<u64, u64>,
    pub per_stream_counts: BTreeMap<String, u64>,
    pub final_state_sha256: Sha256Id,
    pub completeness: bool,
    pub gaps: Vec<String>,
    pub trace_sha256: Sha256Id,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceDocument {
    pub header: TraceHeader,
    pub chunks: Vec<TraceChunk>,
    pub footer: TraceFooter,
}

/// Chunks are canonical JSONL envelopes named by digest under the supplied root.
/// This is an F0 structural encoding, not an adapter that invents absent F1 causes.
pub struct TraceReader {
    document: TraceDocument,
    pub(super) prefix: PrefixDescriptor,
    metadata: ExecutionFile,
    chunks: Vec<ExecutionFile>,
    pub(super) limits: TraceLimits,
    chunk: usize,
    buffer: Vec<u8>,
    at: usize,
    pub(super) count: u64,
    last_id: u64,
    pub(super) events: BTreeMap<u64, EventKey>,
    sequences: BTreeMap<u64, u64>,
    streams: BTreeMap<String, u64>,
    operations: BTreeMap<OperationId, Option<OperationId>>,
    completed_operations: BTreeSet<OperationId>,
    digest: Sha256,
    finished: bool,
    failed: bool,
}
impl TraceReader {
    pub fn open(
        document_path: &Path,
        chunk_root: &Path,
        approved_document_sha256: &Sha256Id,
        prefix: &PrefixDescriptor,
        limits: TraceLimits,
    ) -> Result<Self> {
        limits.validate()?;
        let (metadata, raw) = metadata(document_path, 1_048_576)?;
        require(
            Sha256Id::of(&raw) == *approved_document_sha256,
            "execution.trace_anchor",
        )?;
        let document: TraceDocument = decode(&raw, 1_048_576)?;
        let h = &document.header;
        let f = &document.footer;
        require(
            prefix.projection == Projection::F0StructuralV1
                && h.format == "benilla-reference-trace-v1"
                && h.reference_manifest_sha256 == prefix.reference_manifest_sha256
                && h.schema_sha256 == prefix.schema_sha256
                && h.instrumentation_sha256 == prefix.instrumentation_sha256
                && h.start_boundary == prefix.start
                && f.end_boundary == prefix.end,
            "execution.wrong_prefix",
        )?;
        require(
            prefix.start.per_thread.is_empty(),
            "execution.unsupported_noninitial_prefix",
        )?;
        require(
            f.event_count <= limits.history_events
                && f.per_thread_counts.len() <= limits.threads
                && document.chunks.len() <= 1024,
            "execution.memory_budget",
        )?;
        require(
            f.per_thread_counts == prefix.end.per_thread
                && f.per_thread_counts
                    .values()
                    .try_fold(0_u64, |a, b| a.checked_add(*b))
                    == Some(f.event_count),
            "execution.footer_counts",
        )?;
        require(
            !f.completeness || f.gaps.is_empty(),
            "execution.false_completeness",
        )?;
        let mut chunks = Vec::new();
        let mut prior = None;
        let mut last = 0;
        for (index, descriptor) in document.chunks.iter().enumerate() {
            require(
                descriptor.index == index as u64
                    && descriptor.first_event == last + 1
                    && descriptor.last_event >= descriptor.first_event
                    && descriptor.last_event <= f.event_count
                    && descriptor.previous_chunk_sha256 == prior
                    && descriptor.bytes > 0
                    && descriptor.bytes <= limits.chunk_bytes as u64,
                "execution.chunk_chain",
            )?;
            let receipt = FileReceipt {
                name: descriptor.sha256.0.clone(),
                bytes: descriptor.bytes,
                sha256: descriptor.sha256.clone(),
            };
            chunks.push(receipt.open(chunk_root, limits.chunk_bytes as u64)?);
            last = descriptor.last_event;
            prior = Some(descriptor.sha256.clone());
        }
        require(last == f.event_count, "execution.chunk_count")?;
        Ok(Self {
            prefix: prefix.clone(),
            document,
            metadata,
            chunks,
            limits,
            chunk: 0,
            buffer: Vec::new(),
            at: 0,
            count: 0,
            last_id: 0,
            events: BTreeMap::new(),
            sequences: BTreeMap::new(),
            streams: BTreeMap::new(),
            operations: BTreeMap::new(),
            completed_operations: BTreeSet::new(),
            digest: Sha256::new(),
            finished: false,
            failed: false,
        })
    }
    pub fn document(&self) -> &TraceDocument {
        &self.document
    }
    pub fn next_record(&mut self) -> Result<Option<TraceRecord>> {
        require(!self.failed, "execution.stopped")?;
        let result = self.read_next();
        self.failed |= result.is_err();
        result
    }
    fn read_next(&mut self) -> Result<Option<TraceRecord>> {
        if self.finished {
            return Ok(None);
        }
        self.metadata.check()?;
        if self.at == self.buffer.len() {
            if self.chunk == self.chunks.len() {
                self.finish()?;
                self.finished = true;
                return Ok(None);
            }
            let descriptor = &self.document.chunks[self.chunk];
            self.buffer.resize(descriptor.bytes as usize, 0);
            self.chunks[self.chunk].read_into(0, &mut self.buffer)?;
            require(
                Sha256Id::of(&self.buffer) == descriptor.sha256,
                "execution.chunk_hash",
            )?;
            require(
                self.buffer.last() == Some(&b'\n'),
                "execution.truncated_chunk",
            )?;
            self.digest.update(&self.buffer);
            self.at = 0;
            self.chunk += 1;
        }
        let end = self.buffer[self.at..]
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| refuse("execution.truncated_record"))?
            + self.at;
        require(
            end - self.at <= self.limits.record_bytes,
            "execution.record_limit",
        )?;
        let event: TraceRecord = decode(&self.buffer[self.at..end], self.limits.record_bytes)?;
        self.at = end + 1;
        event.validate()?;
        require(event.id == self.last_id + 1, "execution.event_order")?;
        let descriptor = &self.document.chunks[self.chunk - 1];
        require(
            event.id >= descriptor.first_event
                && event.id <= descriptor.last_event
                && (self.at != self.buffer.len() || event.id == descriptor.last_event),
            "execution.chunk_events",
        )?;
        let sequence = self.sequences.entry(event.thread).or_default();
        require(
            event.thread_sequence == *sequence + 1,
            "execution.thread_order",
        )?;
        *sequence += 1;
        require(
            self.sequences.len() <= self.limits.threads,
            "execution.thread_limit",
        )?;
        for cause in &event.cause_events {
            require(self.events.contains_key(cause), "execution.broken_cause")?;
        }
        if let Some(parent) = event.parent_operation {
            require(
                self.operations.contains_key(&parent),
                "execution.broken_parent",
            )?;
        }
        if let Some(operation) = event.operation {
            match event.kind {
                EventKind::OperationBegin => {
                    require(
                        !self.completed_operations.contains(&operation)
                            && !self.operations.contains_key(&operation)
                            && event.parent_operation != Some(operation),
                        "execution.duplicate_operation",
                    )?;
                    self.operations.insert(operation, event.parent_operation);
                }
                EventKind::OperationEnd => {
                    require(
                        self.operations.get(&operation) == Some(&event.parent_operation)
                            && !self.operations.values().any(|p| *p == Some(operation)),
                        "execution.operation_end",
                    )?;
                    self.operations.remove(&operation);
                    self.completed_operations.insert(operation);
                }
                _ => require(
                    self.operations.get(&operation) == Some(&event.parent_operation),
                    "execution.operation_context",
                )?,
            }
        } else {
            require(
                !matches!(
                    event.kind,
                    EventKind::OperationBegin | EventKind::OperationEnd
                ) && event.parent_operation.is_none(),
                "execution.operation_missing",
            )?;
        }
        if matches!(event.kind, EventKind::RngRaw) {
            let stream = event
                .payload
                .get("stream")
                .ok_or_else(|| refuse("execution.rng_stream"))?;
            require(stream.ty == ScalarType::U64, "execution.rng_stream")?;
            let stream = u64::from_le_bytes(
                stream
                    .bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| refuse("execution.rng_stream"))?,
            );
            *self.streams.entry(stream.to_string()).or_default() += 1;
        }
        self.events.insert(event.id, event.key());
        self.count += 1;
        self.last_id = event.id;
        require(
            self.count <= self.document.footer.event_count,
            "execution.surplus_record",
        )?;
        Ok(Some(event))
    }
    fn finish(&self) -> Result<()> {
        for file in &self.chunks {
            file.check()?;
        }
        let f = &self.document.footer;
        require(
            self.count == f.event_count
                && self.sequences == f.per_thread_counts
                && self.streams == f.per_stream_counts,
            "execution.missing_record",
        )?;
        require(
            format!("{:x}", self.digest.clone().finalize()) == f.trace_sha256.0,
            "execution.trace_hash",
        )
    }
}
