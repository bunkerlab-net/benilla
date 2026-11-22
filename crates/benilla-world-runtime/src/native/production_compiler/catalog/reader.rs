// SPDX-License-Identifier: GPL-3.0-or-later
use super::{check, fail, json, Descriptor, Metrics, Result, CHUNK, FRAME, MAX_INDEX, MAX_RAW};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs::File, path::Path};
#[cfg(windows)]
#[path = "reader_windows.rs"]
mod windows;
#[cfg(windows)]
use windows::Directory;
#[cfg(not(windows))]
type Directory = File;

#[derive(PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    size: u64,
    #[cfg(not(windows))]
    modified: (i64, i64),
    #[cfg(not(windows))]
    changed: (i64, i64),
    #[cfg(windows)]
    modified: u64,
    #[cfg(windows)]
    created: u64,
}
pub(crate) struct Held {
    file: File,
    identity: Identity,
    #[cfg(windows)]
    _directories: Vec<Directory>,
}
pub(super) struct Store {
    directories: Vec<Directory>,
    objects: usize,
    indexes: usize,
    root: usize,
    held: HashMap<(u8, String), Held>,
    index_bytes: u64,
    raw_bytes: u64,
}
#[cfg(unix)]
fn identity(file: &File) -> Result<Identity> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata().map_err(|_| fail("catalog.file"))?;
    check(m.is_file(), "catalog.file")?;
    Ok(Identity {
        device: m.dev(),
        inode: m.ino(),
        size: m.len(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    })
}
#[cfg(windows)]
fn identity(file: &File) -> Result<Identity> {
    windows::identity(file)
}
#[cfg(not(any(unix, windows)))]
fn identity(_: &File) -> Result<Identity> {
    Err(fail("catalog.platform"))
}

#[cfg(unix)]
fn open_root(path: &Path) -> Result<File> {
    use rustix::fs::{open, Mode, OFlags};
    let root = path.canonicalize().map_err(|_| fail("catalog.path"))?;
    let owned = open(
        &root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| fail("catalog.path"))?;
    Ok(File::from(owned))
}
#[cfg(not(any(unix, windows)))]
fn open_root(_: &Path) -> Result<File> {
    Err(fail("catalog.platform"))
}
#[cfg(unix)]
fn open_at(parent: &File, name: &std::ffi::OsStr, directory: bool) -> Result<File> {
    use rustix::fs::{openat, Mode, OFlags};
    let flags = OFlags::RDONLY
        | OFlags::NOFOLLOW
        | OFlags::CLOEXEC
        | OFlags::NONBLOCK
        | if directory {
            OFlags::DIRECTORY
        } else {
            OFlags::empty()
        };
    let owned = openat(parent, name, flags, Mode::empty()).map_err(|_| fail("catalog.path"))?;
    Ok(File::from(owned))
}
#[cfg(not(any(unix, windows)))]
fn open_at(_: &File, _: &std::ffi::OsStr, _: bool) -> Result<File> {
    Err(fail("catalog.platform"))
}
fn open_directory(parent: &Directory, name: &std::ffi::OsStr) -> Result<Directory> {
    #[cfg(windows)]
    {
        windows::directory(parent, name)
    }
    #[cfg(not(windows))]
    {
        open_at(parent, name, true)
    }
}
fn open_child(parent: &Directory, name: &std::ffi::OsStr) -> Result<File> {
    #[cfg(windows)]
    {
        windows::child(parent, name)
    }
    #[cfg(not(windows))]
    {
        open_at(parent, name, false)
    }
}
#[cfg(unix)]
fn read_at(file: &File, out: &mut [u8], offset: u64) -> Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(out, offset)
        .map_err(|_| fail("catalog.truncated"))
}
#[cfg(windows)]
fn read_at(file: &File, out: &mut [u8], offset: u64) -> Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(out, offset)
        .map_err(|_| fail("catalog.truncated"))
}
#[cfg(not(any(unix, windows)))]
fn read_at(_: &File, _: &mut [u8], _: u64) -> Result<usize> {
    Err(fail("catalog.platform"))
}
impl Held {
    pub(crate) fn open(path: &Path, limit: u64) -> Result<Self> {
        #[cfg(windows)]
        {
            let (directories, file) = windows::file(path)?;
            let mut held = Self::new(file, limit)?;
            held._directories = directories;
            Ok(held)
        }
        #[cfg(not(windows))]
        {
            let name = path.file_name().ok_or_else(|| fail("catalog.path"))?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let root = open_root(parent)?;
            Self::new(open_child(&root, name)?, limit)
        }
    }
    pub(crate) fn len(&self) -> u64 {
        self.identity.size
    }
    pub(crate) fn read_into(&self, offset: u64, out: &mut [u8]) -> Result<()> {
        self.check()?;
        check(
            out.len() <= CHUNK && super::add(offset, out.len() as u64)? <= self.len(),
            "catalog.truncated",
        )?;
        check(
            read_at(&self.file, out, offset)? == out.len(),
            "catalog.truncated",
        )?;
        self.check()
    }
    fn new(file: File, limit: u64) -> Result<Self> {
        let identity = identity(&file)?;
        check(identity.size <= limit, "catalog.file")?;
        Ok(Self {
            file,
            identity,
            #[cfg(windows)]
            _directories: Vec::new(),
        })
    }
    pub(crate) fn check(&self) -> Result<()> {
        check(identity(&self.file)? == self.identity, "catalog.changed")
    }
    pub(super) fn read(
        &self,
        offset: u64,
        size: usize,
        metrics: &mut Metrics,
        cache_limit: usize,
    ) -> Result<Vec<u8>> {
        self.check()?;
        check(
            size <= CHUNK && super::add(offset, size as u64)? <= self.identity.size,
            "catalog.truncated",
        )?;
        check(size <= cache_limit, "catalog.cache_limit")?;
        metrics.max_read = metrics.max_read.max(size);
        metrics.reads += 1;
        metrics.bytes += size as u64;
        let mut out = vec![0; size];
        let n = read_at(&self.file, &mut out, offset)?;
        check(n == size, "catalog.truncated")?;
        self.check()?;
        Ok(out)
    }
}
impl Store {
    pub(super) fn open(path: &Path) -> Result<Self> {
        #[cfg(windows)]
        let mut directories = windows::directories(path)?;
        #[cfg(not(windows))]
        let mut directories = vec![open_root(path)?];
        let root = directories.len() - 1;
        let objects = directories.len();
        directories.push(open_directory(
            &directories[root],
            std::ffi::OsStr::new("objects"),
        )?);
        let indexes = directories.len();
        directories.push(open_directory(
            &directories[root],
            std::ffi::OsStr::new("indexes"),
        )?);
        Ok(Self {
            directories,
            objects,
            indexes,
            root,
            held: HashMap::new(),
            index_bytes: 0,
            raw_bytes: 0,
        })
    }
    fn hold(&mut self, namespace: u8, name: &str, limit: u64) -> Result<()> {
        let key = (namespace, name.to_owned());
        if !self.held.contains_key(&key) {
            let parent = match namespace {
                0 => self.objects,
                1 => self.indexes,
                _ => self.root,
            };
            let held = Held::new(
                open_child(&self.directories[parent], std::ffi::OsStr::new(name))?,
                limit,
            )?;
            if namespace == 1 {
                self.index_bytes = super::add(self.index_bytes, held.identity.size)?;
                check(self.index_bytes <= MAX_INDEX as u64, "catalog.index_limit")?;
            } else if namespace == 0 {
                self.raw_bytes = super::add(self.raw_bytes, held.identity.size)?;
                check(self.raw_bytes <= MAX_RAW, "catalog.raw_limit")?;
            }
            self.held.insert(key, held);
        }
        Ok(())
    }
    pub(super) fn named(
        &mut self,
        name: &str,
        limit: usize,
        m: &mut Metrics,
        cache_limit: usize,
    ) -> Result<Vec<u8>> {
        self.hold(2, name, limit as u64)?;
        let held = &self.held[&(2, name.to_owned())];
        held.read(0, held.identity.size as usize, m, cache_limit)
    }
    pub(super) fn check(&self, namespace: u8, h: &str) -> Result<()> {
        self.held
            .get(&(namespace, h.to_owned()))
            .ok_or_else(|| fail("catalog.file"))?
            .check()
    }
    pub(super) fn object(
        &mut self,
        d: &Descriptor,
        namespace: u8,
        m: &mut Metrics,
        cache_limit: usize,
    ) -> Result<Vec<u8>> {
        self.hold(namespace, &d.sha256, CHUNK as u64)?;
        let held = &self.held[&(namespace, d.sha256.clone())];
        check(held.identity.size == d.bytes, "catalog.truncated")?;
        let raw = held.read(0, d.bytes as usize, m, cache_limit)?;
        check(json::hash_bytes(&raw) == d.sha256, "catalog.hash")?;
        Ok(raw)
    }
    pub(super) fn blob(
        &mut self,
        d: &Descriptor,
        m: &mut Metrics,
        cache_limit: usize,
    ) -> Result<()> {
        self.hold(0, &d.sha256, 67_108_864)?;
        let held = &self.held[&(0, d.sha256.clone())];
        check(held.identity.size == d.bytes, "catalog.truncated")?;
        let mut state = Sha256::new();
        let mut at = 0;
        while at < d.bytes {
            let size = (d.bytes - at).min(FRAME as u64) as usize;
            state.update(held.read(at, size, m, cache_limit)?);
            at += size as u64;
        }
        check(
            format!("{:x}", state.finalize()) == d.sha256,
            "catalog.hash",
        )?;
        held.check()
    }
    pub(super) fn blob_range(
        &self,
        d: &Descriptor,
        offset: u64,
        size: usize,
        m: &mut Metrics,
        cache_limit: usize,
    ) -> Result<Vec<u8>> {
        check(size <= FRAME, "catalog.frame_limit")?;
        check(
            super::add(offset, size as u64)? <= d.bytes,
            "catalog.truncated",
        )?;
        let held = self
            .held
            .get(&(0, d.sha256.clone()))
            .ok_or_else(|| fail("catalog.file"))?;
        held.read(offset, size, m, cache_limit)
    }
    pub(super) fn sizes(&self) -> (u64, u64, u64) {
        (
            self.raw_bytes,
            self.index_bytes,
            self.held.values().map(|h| h.identity.size).sum(),
        )
    }
    pub(super) fn check_all(&self) -> Result<()> {
        for held in self.held.values() {
            held.check()?;
        }
        Ok(())
    }
}
pub(super) fn rss_kib() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    text.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")
            .and_then(|s| s.split_whitespace().next()?.parse().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "benilla-catalog-reader-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn refused<T>(result: Result<T>, id: &str) {
        match result {
            Err(error) => assert_eq!(error.id, id),
            Ok(_) => panic!("expected {id}"),
        }
    }
    #[test]
    fn nested_handles_read_positionally_and_keep_limits() {
        let temp = Temp::new();
        let root = temp.0.join("nested");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("objects")).unwrap();
        std::fs::create_dir(root.join("indexes")).unwrap();
        let path = root.join("manifest.json");
        std::fs::write(&path, b"abcdef").unwrap();
        let held = Held::open(&path, 6).unwrap();
        let mut out = [0u8; 3];
        held.read_into(3, &mut out).unwrap();
        assert_eq!(&out, b"def");
        held.read_into(0, &mut out).unwrap();
        assert_eq!(&out, b"abc");
        refused(held.read_into(4, &mut out), "catalog.truncated");
        refused(Held::open(&path, 5), "catalog.file");
        refused(Held::open(&root, 1024), "catalog.file");
        let mut store = Store::open(&root).unwrap();
        assert_eq!(
            store
                .named("manifest.json", 6, &mut Metrics::default(), CHUNK)
                .unwrap(),
            b"abcdef"
        );
        held.check().unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn windows_identity_and_sharing_refuse_mutation_and_replacement() {
        use std::fs::{FileTimes, OpenOptions};
        use std::os::windows::fs::OpenOptionsExt;
        let temp = Temp::new();
        let path = temp.0.join("rows");
        let other = temp.0.join("other");
        std::fs::write(&path, b"same").unwrap();
        std::fs::write(&other, b"same").unwrap();
        let a = identity(&File::open(&path).unwrap()).unwrap();
        let b = identity(&File::open(&other).unwrap()).unwrap();
        assert_ne!((a.device, a.inode), (b.device, b.inode));
        let writer = OpenOptions::new().write(true).open(&path).unwrap();
        refused(Held::open(&path, 4), "catalog.path");
        drop(writer);
        let held = Held::open(&path, 4).unwrap();
        assert!(std::fs::write(&path, b"edit").is_err());
        assert!(std::fs::rename(&path, temp.0.join("moved")).is_err());
        assert!(std::fs::rename(&temp.0, temp.0.with_extension("moved")).is_err());
        let attributes = OpenOptions::new()
            .read(true)
            .access_mode(0x0100)
            .share_mode(7)
            .open(&path)
            .unwrap();
        attributes
            .set_times(
                FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1)),
            )
            .unwrap();
        refused(held.check(), "catalog.changed");
        refused(held.read_into(0, &mut [0u8; 4]), "catalog.changed");
    }
    #[cfg(windows)]
    #[test]
    fn windows_junctions_refuse_in_ancestors_roots_and_object_directories() {
        fn junction(link: &Path, target: &Path) {
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let temp = Temp::new();
        let real = temp.0.join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("rows"), b"data").unwrap();
        let link = temp.0.join("link");
        junction(&link, &real);
        refused(Held::open(&link.join("rows"), 4), "catalog.path");
        refused(Store::open(&link), "catalog.path");
        refused(Held::open(&link, 4), "catalog.path");
        let root = temp.0.join("catalog");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("indexes")).unwrap();
        junction(&root.join("objects"), &real);
        refused(Store::open(&root), "catalog.path");
    }

    #[cfg(unix)]
    #[test]
    fn selected_root_resolves_ancestors_but_descendants_refuse() {
        use std::os::unix::fs::symlink;
        let temp = Temp::new();
        let actual = temp.0.join("actual");
        let root = actual.join("catalog");
        std::fs::create_dir_all(root.join("objects")).unwrap();
        std::fs::create_dir(root.join("indexes")).unwrap();
        std::fs::write(root.join("manifest.json"), b"root").unwrap();
        let alias = temp.0.join("alias");
        symlink(&actual, &alias).unwrap();
        let selected = alias.join("catalog");
        Store::open(&selected).unwrap();
        assert_eq!(
            Held::open(&selected.join("manifest.json"), 4)
                .unwrap()
                .read(0, 4, &mut Metrics::default(), 4)
                .unwrap(),
            b"root"
        );
        symlink(root.join("manifest.json"), root.join("linked")).unwrap();
        refused(Held::open(&selected.join("linked"), 4), "catalog.path");
        std::fs::rename(root.join("objects"), root.join("real-objects")).unwrap();
        symlink(root.join("real-objects"), root.join("objects")).unwrap();
        refused(Store::open(&selected), "catalog.path");
    }
}
