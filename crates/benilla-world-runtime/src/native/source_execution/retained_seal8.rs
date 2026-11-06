// SPDX-License-Identifier: GPL-3.0-or-later
//! Private, hash-anchored seal8-08 diagnostics, not sealed execution inputs.
use super::*;

pub(super) const SEAL8_TRACE_HASH: &str =
    "c1abc189756dede84b46f1e3c4dbafe3c4b4d522fcbd01a4bf59223783612ef2";
const CHUNKS_HASH: &str = "35b0ee2dae897d966581a540183b9e289dd762bb612dbe5b36f5cb9aaad9f405";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {
    bytes: u64,
    first_event: u64,
    index: u64,
    last_event: u64,
    previous_chunk_sha256: Option<Sha256Id>,
    sha256: Sha256Id,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Seal8Context {
    pub thread: u64,
    pub world: u64,
    pub map_update: u64,
    pub map_generation: u64,
    pub map: u32,
    pub instance: u32,
}
// Decode only the retained binary envelope; finalize-seal8-framing.py:20,183,234-253.
pub(super) fn scan_seal8(
    root: &Path,
    mut visit: impl FnMut(Seal8Context, &str, &str) -> Result<()>,
) -> Result<u64> {
    let (held, raw) = metadata(&root.join("trace-chunks.json"), 65_536)?;
    require(
        Sha256Id::of(&raw).as_str() == CHUNKS_HASH,
        "execution.seal8_chunk_manifest",
    )?;
    let chunks: Vec<Chunk> =
        serde_json::from_slice(&raw).map_err(|_| refuse("execution.seal8_manifest_schema"))?;
    require(chunks.len() == 128, "execution.seal8_chunk_count")?;
    let (audit_held, audit_raw) = metadata(&root.join("framing-audit.json"), 65_536)?;
    require(
        Sha256Id::of(&audit_raw).as_str()
            == "4983af878efbe56f1f0420d59c919f98a4f5b0fef42642330836761f352b2664",
        "execution.seal8_audit_identity",
    )?;
    let audit: serde_json::Value =
        serde_json::from_slice(&audit_raw).map_err(|_| refuse("execution.seal8_audit_schema"))?;
    require(
        audit["source_TSV_sha256"].as_str() == Some(SEAL8_TRACE_HASH)
            && audit["reference_execution_sealed"].as_bool() == Some(false),
        "execution.seal8_source_identity",
    )?;
    let mut previous = None;
    let mut event = 0;
    for (index, chunk) in chunks.iter().enumerate() {
        require(
            chunk.index == index as u64
                && chunk.first_event == event + 1
                && chunk.previous_chunk_sha256 == previous
                && chunk.bytes <= 67_108_864,
            "execution.seal8_chunk_chain",
        )?;
        let file = ExecutionFile::open(&root.join(format!("events-{index:04}.bin")), 67_108_864)?;
        require(file.len() == chunk.bytes, "execution.seal8_chunk_length")?;
        let mut bytes = vec![0; chunk.bytes as usize];
        for (part, buffer) in bytes.chunks_mut(1_048_576).enumerate() {
            file.read_into((part * 1_048_576) as u64, buffer)?;
        }
        require(
            Sha256Id::of(&bytes) == chunk.sha256,
            "execution.seal8_chunk_hash",
        )?;
        let mut at = 0;
        while at < bytes.len() {
            let header = bytes
                .get(at..at + 168)
                .ok_or_else(|| refuse("execution.seal8_truncated_header"))?;
            let mut words = [0_u64; 21];
            for (slot, raw) in words.iter_mut().zip(header.as_chunks::<8>().0) {
                *slot = u64::from_le_bytes(*raw);
            }
            let length =
                usize::try_from(words[20]).map_err(|_| refuse("execution.seal8_record_limit"))?;
            require(
                length <= 65_536 && words[0] == event + 1,
                "execution.seal8_record_identity",
            )?;
            let raw = bytes
                .get(at + 168..at + 168 + length)
                .ok_or_else(|| refuse("execution.seal8_truncated_record"))?;
            let text = std::str::from_utf8(raw).map_err(|_| refuse("execution.seal8_utf8"))?;
            let mut fields = text.trim_end_matches('\n').splitn(4, '\t');
            let thread: u64 = parse(fields.next())?;
            let occurrence: u64 = parse(fields.next())?;
            let kind = fields
                .next()
                .ok_or_else(|| refuse("execution.seal8_fields"))?;
            let payload = fields
                .next()
                .ok_or_else(|| refuse("execution.seal8_fields"))?;
            require(
                thread == words[1] && occurrence == words[2] && payload.len() as u64 == words[19],
                "execution.seal8_envelope_identity",
            )?;
            visit(
                Seal8Context {
                    thread,
                    world: words[5],
                    map_update: words[6],
                    map_generation: words[7],
                    map: u32::try_from(words[8])
                        .map_err(|_| refuse("execution.seal8_map_width"))?,
                    instance: u32::try_from(words[9])
                        .map_err(|_| refuse("execution.seal8_map_width"))?,
                },
                kind,
                payload,
            )
            .map_err(|mut reason| {
                reason.id.push_str(&format!(
                    ".event_{}_thread_{thread}_occurrence_{occurrence}",
                    words[0]
                ));
                reason
            })?;
            event += 1;
            at += 168 + length;
        }
        require(event == chunk.last_event, "execution.seal8_chunk_end")?;
        file.check()?;
        previous = Some(chunk.sha256.clone());
    }
    held.check()?;
    audit_held.check()?;
    require(event == 37_984_351, "execution.seal8_trace_identity")?;
    Ok(event)
}
pub(super) fn parse<T: std::str::FromStr>(text: Option<&str>) -> Result<T> {
    text.ok_or_else(|| refuse("execution.seal8_fields"))?
        .parse()
        .map_err(|_| refuse("execution.seal8_number"))
}
