// SPDX-License-Identifier: GPL-3.0-or-later
use super::{check, fail, json, Descriptor, Metrics, Result, CHUNK, FRAME, MAX_INDEX, MAX_RAW};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs::File, path::Path};

#[derive(PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
pub(super) struct Held {
    file: File,
    identity: Identity,
}
pub(super) struct Store {
    directories: Vec<File>,
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
#[cfg(not(unix))]
fn identity(_: &File) -> Result<Identity> {
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
#[cfg(not(unix))]
fn open_at(_: &File, _: &std::ffi::OsStr, _: bool) -> Result<File> {
    Err(fail("catalog.platform"))
}
#[cfg(unix)]
fn read_at(file: &File, out: &mut [u8], offset: u64) -> Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(out, offset)
        .map_err(|_| fail("catalog.truncated"))
}
#[cfg(not(unix))]
fn read_at(_: &File, _: &mut [u8], _: u64) -> Result<usize> {
    Err(fail("catalog.platform"))
}
impl Held {
    pub(super) fn open(path: &Path, limit: u64) -> Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|_| fail("catalog.path"))?
                .join(path)
        };
        let mut parent = File::open("/").map_err(|_| fail("catalog.platform"))?;
        let mut parts = absolute.components().peekable();
        while let Some(part) = parts.next() {
            match part {
                std::path::Component::RootDir => (),
                std::path::Component::Normal(name) => {
                    parent = open_at(&parent, name, parts.peek().is_some())?;
                }
                _ => return Err(fail("catalog.path")),
            }
        }
        Self::new(parent, limit)
    }
    pub(super) fn len(&self) -> u64 {
        self.identity.size
    }
    pub(super) fn read_into(&self, offset: u64, out: &mut [u8]) -> Result<()> {
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
        Ok(Self { file, identity })
    }
    pub(super) fn check(&self) -> Result<()> {
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
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|_| fail("catalog.path"))?
                .join(path)
        };
        let mut directories = vec![File::open("/").map_err(|_| fail("catalog.platform"))?];
        for part in absolute.components() {
            match part {
                std::path::Component::RootDir => (),
                std::path::Component::Normal(name) => {
                    let next = open_at(directories.last().expect("root handle"), name, true)?;
                    directories.push(next);
                }
                _ => return Err(fail("catalog.path")),
            }
        }
        let root = directories.len() - 1;
        let objects = directories.len();
        directories.push(open_at(
            &directories[root],
            std::ffi::OsStr::new("objects"),
            true,
        )?);
        let indexes = directories.len();
        directories.push(open_at(
            &directories[root],
            std::ffi::OsStr::new("indexes"),
            true,
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
                open_at(&self.directories[parent], std::ffi::OsStr::new(name), false)?,
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
