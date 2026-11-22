// SPDX-License-Identifier: GPL-3.0-or-later
//! Retained ancestor handles prevent replacement during Windows path walking.
use super::{check, fail, Identity, Result};
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf, Prefix},
};
const OPEN_REPARSE_POINT: u32 = 0x00200000;
const BACKUP_SEMANTICS: u32 = 0x02000000;
const REPARSE_POINT: u32 = 0x00000400;
const SHARE_READ: u32 = 0x00000001;

pub(super) struct Directory {
    _file: File,
    path: PathBuf,
}

fn component(name: &OsStr) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    check(
        !name.is_empty()
            && !name.encode_wide().any(|c| matches!(c, 0 | 47 | 58 | 92))
            && !matches!(name.encode_wide().last(), Some(32 | 46)),
        "catalog.path",
    )?;
    if let Some(name) = name.to_str() {
        let stem = name.split('.').next().unwrap().trim_end_matches(' ');
        let device = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
            .iter()
            .any(|v| stem.eq_ignore_ascii_case(v));
        let port = stem
            .get(..3)
            .is_some_and(|v| v.eq_ignore_ascii_case("COM") || v.eq_ignore_ascii_case("LPT"))
            && matches!(
                stem.get(3..),
                Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
            );
        check(!device && !port, "catalog.path")?;
    }
    Ok(())
}

fn open(path: &Path, directory: bool) -> Result<File> {
    // Deny write/delete sharing so checked components cannot become reparse points or move.
    let file = OpenOptions::new()
        .read(true)
        .share_mode(SHARE_READ)
        .custom_flags(OPEN_REPARSE_POINT | BACKUP_SEMANTICS)
        .open(path)
        .map_err(|_| fail("catalog.path"))?;
    let metadata = file.metadata().map_err(|_| fail("catalog.file"))?;
    check(
        metadata.file_attributes() & REPARSE_POINT == 0,
        "catalog.path",
    )?;
    check(
        winapi_util::file::typ(&file)
            .map_err(|_| fail("catalog.file"))?
            .is_disk(),
        "catalog.file",
    )?;
    check(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        if directory {
            "catalog.path"
        } else {
            "catalog.file"
        },
    )?;
    Ok(file)
}

pub(super) fn identity(file: &File) -> Result<Identity> {
    let metadata = file.metadata().map_err(|_| fail("catalog.file"))?;
    check(
        metadata.is_file() && metadata.file_attributes() & REPARSE_POINT == 0,
        "catalog.file",
    )?;
    check(
        winapi_util::file::typ(file)
            .map_err(|_| fail("catalog.file"))?
            .is_disk(),
        "catalog.file",
    )?;
    let info = winapi_util::file::information(file).map_err(|_| fail("catalog.file"))?;
    Ok(Identity {
        device: info.volume_serial_number(),
        inode: info.file_index(),
        size: info.file_size(),
        modified: metadata.last_write_time(),
        created: metadata.creation_time(),
    })
}

pub(super) fn directory(parent: &Directory, name: &OsStr) -> Result<Directory> {
    component(name)?;
    let path = parent.path.join(name);
    Ok(Directory {
        _file: open(&path, true)?,
        path,
    })
}

pub(super) fn child(parent: &Directory, name: &OsStr) -> Result<File> {
    component(name)?;
    open(&parent.path.join(name), false)
}

fn walk(path: &Path, final_file: bool) -> Result<(Vec<Directory>, Option<File>)> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| fail("catalog.path"))?
            .join(path)
    };
    let mut parts = absolute.components().peekable();
    let Some(Component::Prefix(prefix)) = parts.next() else {
        return Err(fail("catalog.path"));
    };
    check(
        matches!(
            prefix.kind(),
            Prefix::Disk(_)
                | Prefix::VerbatimDisk(_)
                | Prefix::UNC(_, _)
                | Prefix::VerbatimUNC(_, _)
        ),
        "catalog.path",
    )?;
    check(
        matches!(parts.next(), Some(Component::RootDir)),
        "catalog.path",
    )?;
    let mut root = PathBuf::from(prefix.as_os_str());
    root.push(r"\");
    let mut directories = vec![Directory {
        _file: open(&root, true)?,
        path: root,
    }];
    while let Some(part) = parts.next() {
        let Component::Normal(name) = part else {
            return Err(fail("catalog.path"));
        };
        let parent = directories.last().expect("root handle");
        if final_file && parts.peek().is_none() {
            let file = child(parent, name)?;
            return Ok((directories, Some(file)));
        }
        directories.push(directory(parent, name)?);
    }
    check(!final_file, "catalog.file")?;
    Ok((directories, None))
}

pub(super) fn directories(path: &Path) -> Result<Vec<Directory>> {
    Ok(walk(path, false)?.0)
}

pub(super) fn file(path: &Path) -> Result<(Vec<Directory>, File)> {
    let (directories, file) = walk(path, true)?;
    Ok((directories, file.ok_or_else(|| fail("catalog.file"))?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_names_cannot_alias_devices_streams_or_parent_paths() {
        for name in [
            "..",
            ".. ",
            ".",
            "rows.",
            "rows ",
            "rows:stream",
            "NUL",
            "con.txt",
            "COM1",
            "lpt².txt",
            "a/b",
            "a\\b",
            "a\0b",
        ] {
            assert_eq!(component(OsStr::new(name)).unwrap_err().id, "catalog.path");
        }
        component(OsStr::new("manifest.json")).unwrap();
        component(OsStr::new("normal directory")).unwrap();
        component(OsStr::new("c0ffee")).unwrap();
    }
}
