//! Sidecar storage shares the profile's exclusive lock and audited filesystem primitives.
use super::*;
use crate::world_setup::MAX_SETUP_BYTES;
const CURRENT_SETUP: &str = "local-worlds.json";
const PREVIOUS_SETUP: &str = "local-worlds.previous.json";
impl Storage {
    pub(crate) fn read_world_setups(&mut self) -> Result<Option<Vec<u8>>, Error> {
        self.check_lock()?;
        let file = match open_in(&self.directory, CURRENT_SETUP, Access::Read) {
            Ok(file) => file,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                // A missing current with a saved previous snapshot needs explicit recovery.
                return match open_in(&self.directory, PREVIOUS_SETUP, Access::Read) {
                    Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Ok(_) => Err(Error::Corrupt),
                    Err(error) => Err(error),
                };
            }
            Err(error) => return Err(error),
        };
        if file.metadata()?.len() > MAX_SETUP_BYTES {
            return Err(Error::Corrupt);
        }
        let mut bytes = Vec::new();
        file.take(MAX_SETUP_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_SETUP_BYTES {
            return Err(Error::Corrupt);
        }
        Ok(Some(bytes))
    }
    pub(crate) fn write_world_setups(
        &mut self,
        bytes: &[u8],
        expected: Option<[u8; 32]>,
    ) -> Result<(), Error> {
        let old = self.read_world_setups()?;
        if old.as_ref().map(|old| Sha256::digest(old).into()) != expected {
            return Err(Error::ConcurrentModification);
        }
        if let Some(old) = old {
            self.replace(PREVIOUS_SETUP, &old)?;
        }
        #[cfg(test)]
        if matches!(
            self.fault,
            Some(SaveFault::BeforeSync | SaveFault::BeforeRename)
        ) {
            return Err(std::io::Error::other("injected setup pre-commit failure").into());
        }
        self.replace(CURRENT_SETUP, bytes)?;
        #[cfg(test)]
        if self.fault == Some(SaveFault::AfterRename) {
            return Err(Error::CommitUncertain);
        }
        Ok(())
    }
}
