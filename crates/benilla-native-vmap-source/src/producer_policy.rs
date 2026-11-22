// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS 8ec338a1 flat-output policy, separate from canonical component names.
use crate::{discovery, Error, ModelDependency};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProducerPolicy {
    CanonicalComponent,
    PinnedBasenameFirstWins,
}
impl ProducerPolicy {
    pub const fn writer_policy(self) -> &'static str {
        match self {
            Self::CanonicalComponent => "benilla-vmaps05-canonical-model-path-v9",
            Self::PinnedBasenameFirstWins => "cmangos-8ec338a1-basename-fileexists-first-wins-v1",
        }
    }
    pub const fn listfile_policy(self) -> &'static str {
        match self {
            Self::CanonicalComponent => discovery::LISTFILE_POLICY,
            Self::PinnedBasenameFirstWins => "cmangos-listfile-captured-termination-v1",
        }
    }
    pub(crate) fn key(self, dependency: &ModelDependency) -> std::borrow::Cow<'_, str> {
        match self {
            // Deviation: Key M2 collision by canonical path because CMaNGOS 8ec338a1 gameobject_extract.cpp:26-39 uses basename FileExists first-wins and model.cpp:49-73 writes no zero-hull file, giving a no-collision model the other model's hull; benilla keeps each model's own collision.
            // Deviation: Key WMO geometry by full canonical path because CMaNGOS 8ec338a1 vmapexport.cpp:160-167 flattens basenames and FileExists returns first-wins, then src/game/vmap/TileAssembler.cpp:298-329 writes basename.vmo, giving the loser the winner's geometry and depending on leftover output.
            Self::CanonicalComponent => dependency.member.to_ascii_lowercase().into(),
            // FileExists uses fopen, not the case-insensitive archive hash.
            // gameobject_extract.cpp:26-33; vmapexport.cpp:160-167,91-99.
            Self::PinnedBasenameFirstWins => dependency.extracted_name.as_str().into(),
        }
    }
}

/// Leftover files would seed FileExists before any archive occurrence is visited.
/// We support a fresh output root only, never silently ignore its contents.
pub(crate) fn require_empty_output(path: &Path) -> Result<(), Error> {
    let mut entries = std::fs::read_dir(path).map_err(|_| Error::OutputRootUnavailable)?;
    match entries.next() {
        None => Ok(()),
        Some(Ok(_)) => Err(Error::OutputRootNotEmpty),
        Some(Err(_)) => Err(Error::OutputRootUnavailable),
    }
}

/// model.cpp:241-253,299-300 computes nlen BEFORE shortening .mdx/.mdl.
/// That final NUL is serialized; TileAssembler's C-string output truncates there.
pub(crate) fn doodad_instance_name(raw: &[u8]) -> Result<Vec<u8>, Error> {
    if raw.len() < 4 {
        return Err(Error::DoodadNameIdentity);
    }
    let mut name = raw.rsplit(|&b| b == b'\\').next().unwrap_or(raw).to_vec();
    discovery::fix_name(&mut name);
    discovery::fix_spaces(&mut name);
    if name.ends_with(b".mdx") || name.ends_with(b".mdl") {
        let len = name.len();
        name[len - 2] = b'2';
        name[len - 1] = 0;
    }
    // The caller already validated the captured MODN path.
    Ok(name)
}
