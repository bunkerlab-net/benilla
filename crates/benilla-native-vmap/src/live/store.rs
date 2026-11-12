// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
/// Diagnostic bytes are not newly acquired D1-accepted products.
#[derive(Clone)]
pub enum FileState {
    Absent,
    Present(Arc<[u8]>),
}
#[derive(Default)]
pub struct ProductFiles {
    files: BTreeMap<Vec<u8>, FileState>,
}
impl ProductFiles {
    /// A complete immutable source observation, never an inferred index absence.
    pub fn observe(&mut self, name: impl AsRef<[u8]>, state: FileState) -> Result<()> {
        let name = name.as_ref();
        if let Some(old) = self.files.get(name) {
            let same = match (old, &state) {
                (FileState::Absent, FileState::Absent) => true,
                (FileState::Present(a), FileState::Present(b)) => a == b,
                _ => false,
            };
            if !same {
                return Err(Error::ImmutableSource);
            }
        } else {
            self.files.insert(name.to_vec(), state);
        }
        Ok(())
    }
    pub(super) fn read(&self, name: &[u8]) -> Result<Option<Arc<[u8]>>> {
        // fopen consumes the first NUL; std::string model keys retain all bytes.
        let name = name.split(|&b| b == 0).next().unwrap();
        match self.files.get(name).ok_or(Error::ProductUnavailable)? {
            FileState::Absent => Ok(None),
            FileState::Present(v) => Ok(Some(Arc::clone(v))),
        }
    }
}
