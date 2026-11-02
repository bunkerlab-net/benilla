// SPDX-License-Identifier: GPL-3.0-or-later
use super::{check, fail, Result};
use serde_json::{Map, Number, Value};
use sha2::{Digest as _, Sha256};
use std::{fs::OpenOptions, io::Read, path::Path, str::FromStr};

pub(super) const MAX_DEPTH: usize = 128;
const MAX_NODES: usize = 4_000_000;

pub(super) fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>> {
    #[cfg(not(windows))]
    {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| fail("input.file_unreadable"))?;
        check(metadata.is_file(), "input.regular_file_required")?;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open the final reparse point itself, including directory reparse points.
        // https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
    }
    #[cfg(not(any(unix, windows)))]
    return Err(fail("input.platform_not_supported"));
    let file = options
        .open(path)
        .map_err(|_| fail("input.file_unreadable"))?;
    #[cfg(windows)]
    {
        // std's Windows FileType only distinguishes directories and symlinks.
        // https://doc.rust-lang.org/src/std/sys/fs/windows.rs.html
        // https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfiletype
        let file_type =
            winapi_util::file::typ(&file).map_err(|_| fail("input.regular_file_required"))?;
        check(file_type.is_disk(), "input.regular_file_required")?;
    }
    let metadata = file.metadata().map_err(|_| fail("input.file_unreadable"))?;
    check(metadata.is_file(), "input.regular_file_required")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Reject every reparse tag, not just the tags std recognizes as symlinks.
        // https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x00000400;
        check(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "input.regular_file_required",
        )?;
    }
    let mut data = Vec::with_capacity((metadata.len() as usize).min(limit + 1));
    file.take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| fail("input.file_unreadable"))?;
    check(data.len() <= limit, "json.byte_limit")?;
    Ok(data)
}

/// Matches classic_export.core.canonical for the integer/string source contracts.
pub(super) fn canonical(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("JSON values serialize")
}
pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn digest(value: &impl serde::Serialize) -> String {
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, value).expect("checked JSON serializes");
    format!("{:x}", writer.0.finalize())
}

pub(super) fn bounded_len(
    value: &impl serde::Serialize,
    limit: usize,
    code: &str,
) -> Result<usize> {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit - self.bytes {
                return Err(std::io::ErrorKind::FileTooLarge.into());
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, limit };
    serde_json::to_writer(&mut counter, value).map_err(|_| fail(code))?;
    Ok(counter.bytes)
}

pub(super) fn decode(data: &[u8], limit: usize) -> Result<Value> {
    check(data.len() <= limit, "json.byte_limit")?;
    let mut parser = Parser {
        data,
        at: 0,
        nodes: 0,
    };
    let value = parser.value(0)?;
    parser.space();
    check(parser.at == data.len(), "json.invalid")?;
    Ok(value)
}
struct Parser<'a> {
    data: &'a [u8],
    at: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self
            .data
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\r' | b'\n' | b'\t'))
        {
            self.at += 1;
        }
    }
    fn byte(&mut self, b: u8) -> bool {
        if self.data.get(self.at) == Some(&b) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> Result<String> {
        let start = self.at;
        check(self.byte(b'"'), "json.invalid")?;
        loop {
            match self.data.get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return serde_json::from_slice(&self.data[start..self.at])
                        .map_err(|_| fail("json.invalid"));
                }
                Some(b'\\') => {
                    self.at += 2;
                }
                Some(0..=31) | None => return Err(fail("json.invalid")),
                _ => self.at += 1,
            }
        }
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        check(depth <= MAX_DEPTH, "json.nesting_limit")?;
        self.nodes += 1;
        check(self.nodes <= MAX_NODES, "json.node_limit")?;
        self.space();
        match self.data.get(self.at) {
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b'{') => {
                self.at += 1;
                self.space();
                let mut result = Map::new();
                if self.byte(b'}') {
                    return Ok(Value::Object(result));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    self.space();
                    check(self.byte(b':'), "json.invalid")?;
                    let value = self.value(depth + 1)?;
                    check(!result.contains_key(&key), "json.duplicate_field")?;
                    result.insert(key, value);
                    self.space();
                    if self.byte(b'}') {
                        break;
                    }
                    check(self.byte(b','), "json.invalid")?;
                }
                Ok(Value::Object(result))
            }
            Some(b'[') => {
                self.at += 1;
                self.space();
                let mut result = Vec::new();
                if self.byte(b']') {
                    return Ok(Value::Array(result));
                }
                loop {
                    result.push(self.value(depth + 1)?);
                    self.space();
                    if self.byte(b']') {
                        break;
                    }
                    check(self.byte(b','), "json.invalid")?;
                }
                Ok(Value::Array(result))
            }
            Some(b't') => self.literal(b"true", Value::Bool(true)),
            Some(b'f') => self.literal(b"false", Value::Bool(false)),
            Some(b'n') => self.literal(b"null", Value::Null),
            Some(b'N') if self.data[self.at..].starts_with(b"NaN") => Err(fail("json.nonfinite")),
            Some(b'I') if self.data[self.at..].starts_with(b"Infinity") => {
                Err(fail("json.nonfinite"))
            }
            Some(b'-') if self.data[self.at..].starts_with(b"-Infinity") => {
                Err(fail("json.nonfinite"))
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.at;
                while self
                    .data
                    .get(self.at)
                    .is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
                {
                    self.at += 1;
                }
                let text = std::str::from_utf8(&self.data[start..self.at])
                    .map_err(|_| fail("json.invalid"))?;
                let mut number = Number::from_str(text).map_err(|_| fail("json.invalid"))?;
                if text.contains(['.', 'e', 'E']) {
                    let float: f64 = text.parse().map_err(|_| fail("json.invalid"))?;
                    check(float.is_finite(), "json.nonfinite")?;
                    number = Number::from_f64(float).expect("finite JSON number");
                }
                if text == "-0" {
                    number = Number::from(0);
                }
                Ok(Value::Number(number))
            }
            _ => Err(fail("json.invalid")),
        }
    }
    fn literal(&mut self, text: &[u8], value: Value) -> Result<Value> {
        check(
            self.data.get(self.at..self.at + text.len()) == Some(text),
            "json.invalid",
        )?;
        self.at += text.len();
        Ok(value)
    }
}

pub(super) fn object<'a>(value: &'a Value, code: &str) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| fail(code))
}
pub(super) fn array<'a>(value: &'a Value, code: &str) -> Result<&'a Vec<Value>> {
    value.as_array().ok_or_else(|| fail(code))
}
pub(super) fn string<'a>(value: &'a Value, code: &str) -> Result<&'a str> {
    value.as_str().ok_or_else(|| fail(code))
}
pub(super) fn fields(value: &Value, expected: &[&str], code: &str) -> Result<()> {
    let obj = object(value, code)?;
    check(
        obj.len() == expected.len() && expected.iter().all(|key| obj.contains_key(*key)),
        code,
    )
}
pub(super) fn integer(value: &Value, low: i128, high: i128, code: &str) -> Result<i128> {
    let number = value.as_number().ok_or_else(|| fail(code))?;
    let text = number.as_str();
    check(!text.contains(['.', 'e', 'E']), code)?;
    let value = text.parse::<i128>().map_err(|_| fail(code))?;
    check((low..=high).contains(&value), code)?;
    Ok(value)
}
pub(super) fn int(value: &Value) -> i128 {
    value
        .as_number()
        .and_then(|n| n.as_str().parse().ok())
        .expect("verified integer")
}
