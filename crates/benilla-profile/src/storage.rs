mod world_setup;
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, TryLockError},
    io::{Read, Write},
    path::{Component, PathBuf},
};

#[cfg(not(unix))]
use std::fs::{self, OpenOptions};

const CURRENT: &str = "profile.json";
const PREVIOUS: &str = "profile.previous.json";
const LOCK: &str = "profile.lock";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload<S> {
    secret: Zeroizing<[u8; 32]>,
    public_key: [u8; 32],
    owner_id: OwnerId,
    state: S,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<S> {
    format_version: u32,
    payload: Payload<S>,
    checksum: [u8; 32],
}
// Keep this exact v1 field order: checksum verification precedes conversion to v2.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateV1 {
    revision: u64,
    selected: Option<CharacterId>,
    characters: Vec<Character>,
}
fn checksum<S: Serialize>(payload: &Payload<S>, version: u32) -> Result<[u8; 32], Error> {
    let bytes = Zeroizing::new(serde_json::to_vec(payload).map_err(|_| Error::Corrupt)?);
    let mut hash = Sha256::new();
    hash.update(match version {
        1 => b"benilla-profile-v1\0",
        2 => b"benilla-profile-v2\0",
        _ => return Err(Error::UnsupportedVersion),
    });
    hash.update(&bytes);
    Ok(hash.finalize().into())
}
pub(super) fn encode(
    identity: &ProfileIdentity,
    state: &State,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    let payload = Payload {
        secret: Zeroizing::new(identity.key.to_bytes()),
        public_key: identity.public_key(),
        owner_id: identity.owner_id(),
        state: state.clone(),
    };
    let envelope = Envelope {
        format_version: FORMAT_VERSION,
        checksum: checksum(&payload, FORMAT_VERSION)?,
        payload,
    };
    let bytes = Zeroizing::new(serde_json::to_vec(&envelope).map_err(|_| Error::Corrupt)?);
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Invalid("profile exceeds size limit"));
    }
    Ok(bytes)
}
pub(super) fn decode(bytes: &[u8]) -> Result<(ProfileIdentity, State), Error> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::Corrupt);
    }
    // Read only the discriminator first; errors never echo a key or file contents.
    #[derive(Deserialize)]
    struct Version {
        format_version: u32,
    }
    let version: Version = serde_json::from_slice(bytes).map_err(|_| Error::Corrupt)?;
    let (identity, state) = match version.format_version {
        1 => {
            let envelope: Envelope<StateV1> =
                serde_json::from_slice(bytes).map_err(|_| Error::Corrupt)?;
            let (identity, old) = validate_envelope(envelope)?;
            // Opening is read-only. The next explicit mutation writes v2 atomically and keeps
            // the original validated v1 bytes in profile.previous.json before replacement.
            (
                identity,
                State {
                    revision: old.revision,
                    selected: old.selected,
                    characters: old.characters,
                    archived: Vec::new(),
                },
            )
        }
        FORMAT_VERSION => validate_envelope(
            serde_json::from_slice::<Envelope<State>>(bytes).map_err(|_| Error::Corrupt)?,
        )?,
        _ => return Err(Error::UnsupportedVersion),
    };
    state.validate(identity.owner_id())?;
    Ok((identity, state))
}
fn validate_envelope<S: Serialize>(envelope: Envelope<S>) -> Result<(ProfileIdentity, S), Error> {
    if checksum(&envelope.payload, envelope.format_version)? != envelope.checksum {
        return Err(Error::Corrupt);
    }
    let identity = ProfileIdentity {
        key: SigningKey::from_bytes(&envelope.payload.secret),
    };
    if identity.public_key() != envelope.payload.public_key
        || identity.owner_id() != envelope.payload.owner_id
    {
        return Err(Error::Corrupt);
    }
    Ok((identity, envelope.payload.state))
}

pub(super) struct Storage {
    #[cfg(unix)]
    directory: File,
    #[cfg(not(unix))]
    directory: PathBuf,
    lock: File,
    locked: bool,
    #[cfg(test)]
    fault: Option<SaveFault>,
}
impl Storage {
    pub(super) fn acquire(path: &Path, create: bool) -> Result<Self, Error> {
        let directory = open_directory(path, create)?;
        let mut value = Self {
            lock: open_in(&directory, LOCK, Access::Lock)?,
            directory,
            locked: false,
            #[cfg(test)]
            fault: None,
        };
        match value.lock.try_lock() {
            Ok(()) => value.locked = true,
            Err(TryLockError::WouldBlock) => return Err(Error::Locked),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        // The lock file is persistent, never unlinked on close (avoids split-lock races).
        value.check_lock()?;
        Ok(value)
    }
    fn check_lock(&mut self) -> Result<(), Error> {
        let named = open_in(&self.directory, LOCK, Access::Read)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let a = named.metadata()?;
            let b = self.lock.metadata()?;
            if a.ino() != b.ino() || a.dev() != b.dev() {
                return Err(Error::ConcurrentModification);
            }
        }
        #[cfg(not(unix))]
        let _ = named;
        Ok(())
    }
    pub(super) fn ensure_absent(&self) -> Result<(), Error> {
        for name in [CURRENT, PREVIOUS] {
            match open_in(&self.directory, name, Access::Read) {
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => return Err(Error::AlreadyExists),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    pub(super) fn read(&self) -> Result<Zeroizing<Vec<u8>>, Error> {
        let file = open_in(&self.directory, CURRENT, Access::Read)?;
        if file.metadata()?.len() > MAX_FILE_BYTES {
            return Err(Error::Corrupt);
        }
        let mut bytes = Zeroizing::new(Vec::new());
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Corrupt);
        }
        Ok(bytes)
    }
    fn verify_lock(&self) -> Result<(), Error> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let named = open_in(&self.directory, LOCK, Access::Read)?.metadata()?;
            let held = self.lock.metadata()?;
            if named.ino() != held.ino() || named.dev() != held.dev() {
                return Err(Error::ConcurrentModification);
            }
        }
        Ok(())
    }
    /// Read-only checkpoint verification, including the held lock's identity.
    pub(super) fn verified_current(&self, expected: [u8; 32]) -> Result<Zeroizing<Vec<u8>>, Error> {
        self.verify_lock()?;
        let old = self.read()?;
        if <[u8; 32]>::from(Sha256::digest(&old)) != expected {
            return Err(Error::ConcurrentModification);
        }
        decode(&old)?;
        Ok(old)
    }
    pub(super) fn write(&self, bytes: &[u8], expected: Option<[u8; 32]>) -> Result<(), Error> {
        self.verify_lock()?;
        if let Some(expected) = expected {
            let old = self.verified_current(expected)?;
            // Preserve the last fully valid current snapshot before replacing it.
            self.replace(PREVIOUS, &old)?;
        } else {
            self.ensure_absent()?;
        }
        self.replace(CURRENT, bytes)
    }
    #[cfg(test)]
    pub(super) fn clone_lock(&self) -> File {
        self.lock.try_clone().unwrap()
    }
    #[cfg(test)]
    pub(super) fn inject_fault(&mut self, fault: SaveFault) {
        self.fault = Some(fault);
    }
    fn replace(&self, target: &str, bytes: &[u8]) -> Result<(), Error> {
        // Refuse existing symlinks/hardlinks or permissive files, including the backup.
        match open_in(&self.directory, target, Access::Read) {
            Ok(_) => {}
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce)
            .map_err(|_| Error::Invalid("secure random source unavailable"))?;
        let suffix: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let name = format!(".profile-{suffix}.tmp");
        let result = (|| {
            let mut file = open_in(&self.directory, &name, Access::New)?;
            file.write_all(bytes)?;
            #[cfg(test)]
            if target == CURRENT && self.fault == Some(SaveFault::BeforeSync) {
                return Err(std::io::Error::other("injected temp-file sync failure").into());
            }
            file.flush()?;
            file.sync_all()?;
            drop(file);
            #[cfg(test)]
            if target == CURRENT && self.fault == Some(SaveFault::BeforeRename) {
                return Err(std::io::Error::other("injected pre-rename failure").into());
            }
            rename_in(&self.directory, &name, target)?;
            #[cfg(test)]
            if target == CURRENT && self.fault == Some(SaveFault::AfterRename) {
                return Err(Error::CommitUncertain);
            }
            sync_directory(&self.directory).map_err(|_| Error::CommitUncertain)
        })();
        // After successful rename this is absent; a failed write leaves current untouched.
        let _ = remove_in(&self.directory, &name);
        result
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        // A concurrent fork can briefly inherit this open-file description until exec,
        // despite CLOEXEC. Explicit unlock permits immediate close/reopen in that window.
        if self.locked {
            let _ = self.lock.unlock();
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveFault {
    BeforeSync,
    BeforeRename,
    AfterRename,
}

#[derive(Clone, Copy)]
enum Access {
    Read,
    Lock,
    New,
}

fn absolute_parts(path: &Path) -> Result<PathBuf, Error> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    // A Windows drive-relative prefix (C:profile) replaces the base rather than joining it.
    if !absolute.is_absolute() {
        return Err(Error::Invalid(
            "profile path must resolve to an absolute path",
        ));
    }
    if absolute
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(Error::Invalid(
            "profile path must not contain parent traversal",
        ));
    }
    if absolute.file_name().is_none() {
        return Err(Error::Invalid("profile requires a dedicated directory"));
    }
    Ok(absolute)
}

#[cfg(unix)]
fn c_name(name: &std::ffi::OsStr) -> Result<std::ffi::CString, Error> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(name.as_bytes()).map_err(|_| Error::Invalid("invalid profile path"))
}
#[cfg(unix)]
fn open_directory(path: &Path, create: bool) -> Result<File, Error> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::MetadataExt;
    let absolute = absolute_parts(path)?;
    let parts: Vec<_> = absolute
        .components()
        .filter_map(|p| match p {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let mut directory = File::open("/")?;
    for (index, part) in parts.iter().enumerate() {
        let name = c_name(part)?;
        let last = index + 1 == parts.len();
        if last && create {
            // SAFETY: live directory descriptor and NUL-terminated name; no pointers escape.
            let result = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(error.into());
                }
            } else {
                directory.sync_all()?;
            }
        }
        // Open every component relative to its held parent; no symlink/path-swap traversal.
        // SAFETY: same as mkdirat; successful fd is uniquely owned by the resulting File.
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: openat just returned this owned descriptor, transferred exactly once.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    let metadata = directory.metadata()?;
    // SAFETY: geteuid has no pointer or ownership requirements.
    if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(Error::Invalid(
            "profile directory must be owned by this user with mode 0700",
        ));
    }
    Ok(directory)
}
#[cfg(unix)]
fn open_in(directory: &File, name: &str, access: Access) -> Result<File, Error> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::MetadataExt;
    let name = c_name(std::ffi::OsStr::new(name))?;
    let flags = match access {
        Access::Read => libc::O_RDONLY,
        Access::Lock => libc::O_RDWR | libc::O_CREAT,
        Access::New => libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
    };
    // NONBLOCK makes a malicious FIFO fail validation rather than hanging before fstat.
    // SAFETY: held descriptor, NUL name, fixed flags, restrictive mode; returned fd is owned.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: owned descriptor from successful openat, transferred exactly once.
    let file = unsafe { File::from_raw_fd(fd) };
    let metadata = file.metadata()?;
    // SAFETY: geteuid has no pointer or ownership requirements.
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(Error::Invalid(
            "profile file must be a private, singly-linked regular file",
        ));
    }
    Ok(file)
}
#[cfg(unix)]
fn rename_in(directory: &File, source: &str, target: &str) -> Result<(), Error> {
    use std::os::fd::AsRawFd;
    let source = c_name(std::ffi::OsStr::new(source))?;
    let target = c_name(std::ffi::OsStr::new(target))?;
    // SAFETY: both names and the directory descriptor remain live for the entire call.
    if unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            source.as_ptr(),
            directory.as_raw_fd(),
            target.as_ptr(),
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(unix)]
fn remove_in(directory: &File, name: &str) -> Result<(), Error> {
    use std::os::fd::AsRawFd;
    let name = c_name(std::ffi::OsStr::new(name))?;
    // SAFETY: a live directory fd and NUL-terminated child filename; never recursive.
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(unix)]
fn sync_directory(directory: &File) -> Result<(), Error> {
    directory.sync_all().map_err(Error::Io)
}

#[cfg(not(unix))]
fn check_component_paths(
    path: &Path,
    mut check: impl FnMut(&Path) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        // A prefix is not a directory: wait for RootDir to complete C:\ or \\?\C:\.
        // UNC prefixes likewise become a complete share root before any metadata query.
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        check(&current)?;
    }
    Ok(())
}
#[cfg(not(unix))]
fn check_components(path: &Path) -> Result<(), Error> {
    check_component_paths(path, |current| {
        let metadata = fs::symlink_metadata(current)?;
        if metadata.file_type().is_symlink() {
            return Err(Error::Invalid("profile path contains a symlink"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            // Includes junctions and other reparse points, not just symbolic links.
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(Error::Invalid("profile path contains a reparse point"));
            }
        }
        Ok(())
    })
}
#[cfg(not(unix))]
fn open_directory(path: &Path, create: bool) -> Result<PathBuf, Error> {
    let path = absolute_parts(path)?;
    let parent = path
        .parent()
        .ok_or(Error::Invalid("profile requires a parent directory"))?;
    check_components(parent)?;
    if create {
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    check_components(&path)?;
    if !path.is_dir() {
        return Err(Error::Invalid("profile path is not a directory"));
    }
    Ok(path)
}
#[cfg(not(unix))]
fn open_in(directory: &Path, name: &str, access: Access) -> Result<File, Error> {
    check_components(directory)?;
    let path = directory.join(name);
    match fs::symlink_metadata(&path) {
        Ok(_) => check_components(&path)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut options = OpenOptions::new();
    match access {
        Access::Read => {
            options.read(true);
        }
        Access::Lock => {
            options.read(true).write(true).create(true);
        }
        Access::New => {
            options.write(true).create_new(true);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open the reparse point itself and reject its metadata; never follow it.
        options.custom_flags(0x00200000);
        if matches!(access, Access::Lock) {
            // Allow readers/writers to contend on the OS lock, but never delete/replace it.
            options.share_mode(0x1 | 0x2);
        }
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::Invalid("profile file is not a regular file"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if file.metadata()?.file_attributes() & 0x400 != 0 {
            return Err(Error::Invalid("profile file is a reparse point"));
        }
    }
    Ok(file)
}
#[cfg(not(unix))]
fn rename_in(directory: &Path, source: &str, target: &str) -> Result<(), Error> {
    check_components(directory)?;
    fs::rename(directory.join(source), directory.join(target)).map_err(Error::Io)
}
#[cfg(not(unix))]
fn remove_in(directory: &Path, name: &str) -> Result<(), Error> {
    fs::remove_file(directory.join(name)).map_err(Error::Io)
}
#[cfg(not(unix))]
fn sync_directory(_: &Path) -> Result<(), Error> {
    // Windows Rust std cannot portably fsync a directory. File data is flushed before the
    // rename; directory-entry durability through sudden power loss is not promised.
    Ok(())
}

#[cfg(all(test, windows))]
mod windows_path_tests {
    use super::*;

    #[test]
    fn component_checks_start_at_complete_drive_or_share_roots() {
        for root in [
            r"C:\",
            r"\\?\C:\",
            r"\\server\share\",
            r"\\?\UNC\server\share\",
        ] {
            let root = Path::new(root);
            for path in [root.to_path_buf(), root.join(r"profiles\alice")] {
                let mut checked = Vec::new();
                check_component_paths(&path, |part| {
                    assert!(part.is_absolute());
                    checked.push(part.to_path_buf());
                    Ok(())
                })
                .unwrap();
                let expected = if path == root {
                    vec![root.to_path_buf()]
                } else {
                    vec![root.to_path_buf(), root.join("profiles"), path]
                };
                assert_eq!(checked, expected);
            }
        }
    }

    #[test]
    fn drive_relative_profile_paths_are_rejected() {
        assert!(matches!(
            absolute_parts(Path::new(r"C:profile")),
            Err(Error::Invalid(_))
        ));
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    fn fixture() -> Envelope<State> {
        let identity = ProfileIdentity {
            key: SigningKey::from_bytes(&[5; 32]),
        };
        let character = Character {
            id: [1; 16],
            owner_id: identity.owner_id(),
            name: "Fixture".into(),
            race: 1,
            class: 1,
            gender: 0,
            appearance: Appearance::default(),
            level: 1,
            spawn_policy: SpawnPolicy::NorthshireMeetingV1,
            position: SpawnPolicy::NorthshireMeetingV1.position().unwrap(),
        };
        let state = State {
            revision: 0,
            selected: Some(character.id),
            characters: vec![character],
            archived: Vec::new(),
        };
        serde_json::from_slice(&encode(&identity, &state).unwrap()).unwrap()
    }
    #[test]
    fn semantic_validation_survives_recomputed_checksums() {
        for case in 0..9 {
            let mut envelope = fixture();
            match case {
                0 => envelope.payload.owner_id = [9; 16],
                1 => envelope.payload.public_key = [9; 32],
                2 => envelope.payload.state.characters[0].owner_id = [9; 16],
                3 => envelope.payload.state.characters[0].level = 60,
                4 => envelope.payload.state.characters[0].id = [0; 16],
                5 => envelope.payload.state.characters[0].class = 7,
                6 => envelope.payload.state.selected = Some([9; 16]),
                7 => {
                    let cloned = envelope.payload.state.characters[0].clone();
                    envelope.payload.state.characters.push(cloned);
                }
                8 => envelope.payload.state.characters[0].name = "fIxTuRe".into(),
                _ => unreachable!(),
            }
            envelope.checksum = checksum(&envelope.payload, envelope.format_version).unwrap();
            let bytes = Zeroizing::new(serde_json::to_vec(&envelope).unwrap());
            assert!(matches!(decode(&bytes), Err(Error::Corrupt)), "case {case}");
        }
    }
    #[test]
    fn valid_encoding_is_stable_and_checksum_detects_valid_json_corruption() {
        let mut envelope = fixture();
        let bytes = Zeroizing::new(serde_json::to_vec(&envelope).unwrap());
        let (identity, state) = decode(&bytes).unwrap();
        assert_eq!(*encode(&identity, &state).unwrap(), *bytes);
        envelope.payload.state.characters[0].name = "Altered".into();
        assert!(matches!(
            decode(&serde_json::to_vec(&envelope).unwrap()),
            Err(Error::Corrupt)
        ));
    }
}
