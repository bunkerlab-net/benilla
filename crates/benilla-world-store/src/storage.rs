// SPDX-License-Identifier: GPL-3.0-or-later
// Filesystem primitives adapted from benilla-profile (MIT OR Apache-2.0).
use super::*;
#[cfg(not(unix))]
use std::fs::{self, OpenOptions};
use std::{
    fs::{File, TryLockError},
    io::{Read, Write},
    path::{Component, PathBuf},
};

pub(super) const CURRENT: &str = "world.checkpoint";
pub(super) const PREVIOUS: &str = "world.previous";
pub(super) const LOCK: &str = "world.lock";
pub(super) const TEMP: &str = ".world-checkpoint.pending";

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
    pub fn acquire(path: &Path, create: bool) -> Result<Self, Error> {
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
        value.check_lock()?;
        Ok(value)
    }
    fn check_lock(&self) -> Result<(), Error> {
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
    fn ensure_absent(&self) -> Result<(), Error> {
        for name in [CURRENT, PREVIOUS, TEMP] {
            match open_in(&self.directory, name, Access::Read) {
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => return Err(Error::AlreadyExists),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    pub fn read(&self) -> Result<Vec<u8>, Error> {
        self.read_named(CURRENT)
    }
    pub fn read_previous(&self) -> Result<Vec<u8>, Error> {
        self.read_named(PREVIOUS)
    }
    fn read_named(&self, name: &str) -> Result<Vec<u8>, Error> {
        let file = open_in(&self.directory, name, Access::Read)?;
        if file.metadata()?.len() > MAX_CHECKPOINT_BYTES as u64 {
            return Err(Error::Corrupt);
        }
        let mut bytes = Vec::new();
        file.take(MAX_CHECKPOINT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(Error::Corrupt);
        }
        Ok(bytes)
    }
    pub fn write(
        &self,
        bytes: &[u8],
        expected: Option<(Digest, Option<Digest>)>,
    ) -> Result<(), Error> {
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(Error::Corrupt);
        }
        self.check_lock()?;
        if let Some((expected, previous)) = expected {
            let old = self.read().map_err(|_| Error::ConcurrentModification)?;
            if hash(&old) != expected {
                return Err(Error::ConcurrentModification);
            }
            match self.read_previous() {
                Ok(backup) => {
                    let digest = hash(&backup);
                    // A prior failed backup-first attempt may already have copied current.
                    if Some(digest) != previous && digest != expected {
                        return Err(Error::ConcurrentModification);
                    }
                }
                Err(Error::Io(e))
                    if e.kind() == std::io::ErrorKind::NotFound && previous.is_none() => {}
                Err(_) => return Err(Error::ConcurrentModification),
            }
            Envelope::decode(&old)?;
            // A separate replacement; there is deliberately no multi-file atomicity claim.
            self.replace(PREVIOUS, &old)?;
        } else {
            self.ensure_absent()?;
        }
        self.replace(CURRENT, bytes)?;
        #[cfg(test)]
        if self.fault == Some(SaveFault::BeforeVerification) {
            return Err(Error::CommitUncertain);
        }
        // A receipt is minted only for the exact bytes read back through the held directory.
        if self.read().map_err(|_| Error::CommitUncertain)? != bytes {
            return Err(Error::CommitUncertain);
        }
        Ok(())
    }
    fn replace(&self, target: &str, bytes: &[u8]) -> Result<(), Error> {
        match open_in(&self.directory, target, Access::Read) {
            Ok(_) => {}
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        // One fixed-size staging slot bounds crash debris. A stale pending file is never
        // deleted/reused implicitly; preserving it requires explicit local recovery.
        let mut file = open_in(&self.directory, TEMP, Access::New)?;
        let result = (|| {
            #[cfg(test)]
            if target == CURRENT && self.fault == Some(SaveFault::PartialWrite) {
                file.write_all(&bytes[..bytes.len() / 2])?;
                return Err(std::io::Error::other("injected partial write").into());
            }
            file.write_all(bytes)?;
            #[cfg(test)]
            if target == CURRENT && self.fault == Some(SaveFault::BeforeSync) {
                return Err(std::io::Error::other("injected file sync failure").into());
            }
            file.flush()?;
            file.sync_all()?;
            drop(file);
            #[cfg(test)]
            if (target == CURRENT && self.fault == Some(SaveFault::BeforeRename))
                || (target == PREVIOUS && self.fault == Some(SaveFault::BeforePreviousRename))
            {
                return Err(std::io::Error::other("injected pre-rename failure").into());
            }
            // Conservatively treat a failed rename as ambiguous on unusual filesystems.
            rename_in(&self.directory, TEMP, target).map_err(|_| Error::CommitUncertain)?;
            #[cfg(test)]
            if (target == CURRENT && self.fault == Some(SaveFault::AfterRename))
                || (target == PREVIOUS && self.fault == Some(SaveFault::AfterPreviousRename))
            {
                return Err(Error::CommitUncertain);
            }
            sync_directory(&self.directory).map_err(|_| Error::CommitUncertain)
        })();
        // Only clean a staging file that this call actually created. Never remove someone
        // else's pre-existing pending file when create_new failed.
        let _ = remove_in(&self.directory, TEMP);
        result
    }
    #[cfg(test)]
    pub fn clone_lock(&self) -> File {
        self.lock.try_clone().unwrap()
    }
    #[cfg(test)]
    pub fn clear_fault(&mut self) {
        self.fault = None;
    }
    #[cfg(test)]
    pub fn inject_fault(&mut self, fault: SaveFault) {
        self.fault = Some(fault);
    }
}
impl Drop for Storage {
    fn drop(&mut self) {
        // A concurrent fork can briefly inherit the open-file description until exec,
        // despite CLOEXEC. Explicit unlock avoids a false Locked immediately after close.
        if self.locked {
            let _ = self.lock.unlock();
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SaveFault {
    PartialWrite,
    BeforeSync,
    BeforeRename,
    AfterRename,
    BeforePreviousRename,
    AfterPreviousRename,
    BeforeVerification,
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
    // A Windows drive-relative prefix (C:world) replaces the base rather than joining it.
    if !absolute.is_absolute() {
        return Err(Error::Invalid(
            "world path must resolve to an absolute path",
        ));
    }
    if absolute
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(Error::Invalid(
            "world path must not contain parent traversal",
        ));
    }
    if absolute.components().any(|part| matches!(part, Component::Normal(name) if name.to_string_lossy().eq_ignore_ascii_case("p2p-profile"))) {
        return Err(Error::Invalid("world checkpoints require a separate directory from p2p-profile"));
    }
    if absolute.file_name().is_none() {
        return Err(Error::Invalid("world requires a dedicated directory"));
    }
    Ok(absolute)
}

#[cfg(unix)]
fn c_name(name: &std::ffi::OsStr) -> Result<std::ffi::CString, Error> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(name.as_bytes()).map_err(|_| Error::Invalid("invalid world path"))
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
            "world directory must be owned by this user with mode 0700",
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
            "world file must be a private, singly-linked regular file",
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
            return Err(Error::Invalid("world path contains a symlink"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            // Includes junctions and other reparse points, not just symbolic links.
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(Error::Invalid("world path contains a reparse point"));
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
        .ok_or(Error::Invalid("world requires a parent directory"))?;
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
        return Err(Error::Invalid("world path is not a directory"));
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
        return Err(Error::Invalid("world file is not a regular file"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if file.metadata()?.file_attributes() & 0x400 != 0 {
            return Err(Error::Invalid("world file is a reparse point"));
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
    check_components(directory)?;
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
            for path in [root.to_path_buf(), root.join(r"worlds\alice")] {
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
                    vec![root.to_path_buf(), root.join("worlds"), path]
                };
                assert_eq!(checked, expected);
            }
        }
    }

    #[test]
    fn drive_relative_world_paths_are_rejected() {
        assert!(matches!(
            absolute_parts(Path::new(r"C:world")),
            Err(Error::Invalid(_))
        ));
    }
}
