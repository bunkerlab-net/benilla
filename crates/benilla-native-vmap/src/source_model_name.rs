// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

use sha2::{Digest, Sha256};

const MAX_NAME_BYTES: usize = 500;
const DIGEST_DOMAIN: &[u8] = b"benilla-source-model-name-v1\0";
const SAFE_BASENAME_BYTES: usize = 7 + 64 + 4;

/// A bounded source model-name identity, including its optional terminal NUL.
///
/// Equality and ordering use the complete, unmodified bytes. Case, punctuation,
/// whitespace, and the presence of a terminal NUL are never normalized. Raw and
/// effective source basenames describe source behavior; only [`Self::safe_basename`]
/// produces a portable filename for a separately managed output directory.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceModelName(Vec<u8>);

/// A rejected source model-name identity or a failed bounded allocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceModelNameError {
    InputLimit { bytes: usize, maximum: usize },
    EmptyBody,
    EmbeddedNul { offset: usize },
    NonPrintableByte { offset: usize, byte: u8 },
    PathSeparator { offset: usize, byte: u8 },
    ReservedBasename,
    AllocationFailed,
}

impl SourceModelName {
    /// Validate and copy at most 500 full bytes, including an optional final NUL.
    ///
    /// The body must be nonempty printable ASCII, without `/`, `\\`, or `:`.
    /// The basenames `.` and `..` and any embedded NUL are rejected. This method
    /// performs no filesystem access or platform-specific name normalization.
    pub fn new(bytes: &[u8]) -> Result<Self, SourceModelNameError> {
        if bytes.len() > MAX_NAME_BYTES {
            return Err(SourceModelNameError::InputLimit {
                bytes: bytes.len(),
                maximum: MAX_NAME_BYTES,
            });
        }
        let body = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        if body.is_empty() {
            return Err(SourceModelNameError::EmptyBody);
        }
        for (offset, &byte) in body.iter().enumerate() {
            if byte == 0 {
                return Err(SourceModelNameError::EmbeddedNul { offset });
            }
            if !(b' '..=b'~').contains(&byte) {
                return Err(SourceModelNameError::NonPrintableByte { offset, byte });
            }
            if matches!(byte, b'/' | b'\\' | b':') {
                return Err(SourceModelNameError::PathSeparator { offset, byte });
            }
        }
        if matches!(body, b"." | b"..") {
            return Err(SourceModelNameError::ReservedBasename);
        }
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(bytes.len())
            .map_err(|_| SourceModelNameError::AllocationFailed)?;
        owned.extend_from_slice(bytes);
        Ok(Self(owned))
    }

    /// The exact validated bytes, including the terminal NUL when present.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The source basename with only its optional terminal NUL stripped.
    pub fn raw_basename(&self) -> &[u8] {
        self.0.strip_suffix(&[0]).unwrap_or(&self.0)
    }

    /// The basename observed by the source's C-string file-open boundary.
    ///
    /// The source appends `.vmo` to its full string before passing `c_str()` to
    /// the file API: an existing terminal NUL hides the appended suffix. These
    /// bytes describe that behavior and must not be used as a safe output path.
    pub fn effective_source_basename(&self) -> Result<Vec<u8>, SourceModelNameError> {
        let body = self.raw_basename();
        let suffix: &[u8] = if self.0.last() == Some(&0) {
            b""
        } else {
            b".vmo"
        };
        let mut basename = Vec::new();
        // The validated full name is at most 500 bytes; the suffix is at most 4.
        basename
            .try_reserve_exact(body.len() + suffix.len())
            .map_err(|_| SourceModelNameError::AllocationFailed)?;
        basename.extend_from_slice(body);
        basename.extend_from_slice(suffix);
        Ok(basename)
    }

    /// A portable `source-<64 lowercase hex digits>.vmo` identity mapping.
    ///
    /// SHA-256 covers `benilla-source-model-name-v1\0`, the full byte length as
    /// little-endian `u32`, and the full name, including any terminal NUL. This
    /// mapping neither opens a file nor proves that a model exists or is usable.
    pub fn safe_basename(&self) -> Result<String, SourceModelNameError> {
        let mut hasher = Sha256::new();
        hasher.update(DIGEST_DOMAIN);
        hasher.update((self.0.len() as u32).to_le_bytes());
        hasher.update(&self.0);
        let digest = hasher.finalize();
        let mut basename = String::new();
        basename
            .try_reserve_exact(SAFE_BASENAME_BYTES)
            .map_err(|_| SourceModelNameError::AllocationFailed)?;
        basename.push_str("source-");
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in digest {
            basename.push(char::from(HEX[usize::from(byte >> 4)]));
            basename.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        basename.push_str(".vmo");
        debug_assert_eq!(basename.len(), SAFE_BASENAME_BYTES);
        Ok(basename)
    }
}

impl fmt::Display for SourceModelNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit { bytes, maximum } => {
                write!(
                    f,
                    "source model-name length {bytes} exceeds limit {maximum}"
                )
            }
            Self::EmptyBody => f.write_str("source model-name body is empty"),
            Self::EmbeddedNul { offset } => {
                write!(
                    f,
                    "source model name contains an embedded NUL at byte {offset}"
                )
            }
            Self::NonPrintableByte { offset, byte } => {
                write!(
                    f,
                    "source model-name byte {offset} is non-printable ASCII: {byte}"
                )
            }
            Self::PathSeparator { offset, byte } => {
                write!(
                    f,
                    "source model-name byte {offset} is a path separator: {byte}"
                )
            }
            Self::ReservedBasename => f.write_str("source model-name body is . or .."),
            Self::AllocationFailed => f.write_str("source model-name allocation failed"),
        }
    }
}

impl std::error::Error for SourceModelNameError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn identity_preserves_case_terminal_nul_and_exact_bytes() {
        let inputs: &[&[u8]] = &[b"Foo.m2", b"Foo.m2\0", b"foo.m2", b"foo.m2\0"];
        let names: Vec<_> = inputs
            .iter()
            .map(|bytes| SourceModelName::new(bytes).unwrap())
            .collect();
        for (name, &input) in names.iter().zip(inputs) {
            assert_eq!(name.as_bytes(), input);
            assert_eq!(name, &name.clone());
            assert_eq!(
                name.raw_basename(),
                input.strip_suffix(&[0]).unwrap_or(input)
            );
        }
        let ordered: BTreeSet<_> = names.iter().cloned().collect();
        assert_eq!(ordered.len(), inputs.len());
        assert_eq!(ordered.into_iter().collect::<Vec<_>>(), names);
        let safe_names: BTreeSet<_> = names
            .iter()
            .map(|name| name.safe_basename().unwrap())
            .collect();
        assert_eq!(safe_names.len(), inputs.len());
    }

    #[test]
    fn constructor_owns_the_input_without_normalizing_it() {
        let mut bytes = b"  CON .M2 \0".to_vec();
        let name = SourceModelName::new(&bytes).unwrap();
        bytes.fill(b'x');
        assert_eq!(name.as_bytes(), b"  CON .M2 \0");
        assert_eq!(name.raw_basename(), b"  CON .M2 ");
    }

    #[test]
    fn effective_basename_matches_terminal_nul_file_open_semantics() {
        let cases: &[(&[u8], &[u8])] = &[
            (b"Foo.m2", b"Foo.m2.vmo"),
            (b"Foo.m2\0", b"Foo.m2"),
            (b"Foo.m2.vmo", b"Foo.m2.vmo.vmo"),
            (b"Foo.m2.vmo\0", b"Foo.m2.vmo"),
            (b"a ", b"a .vmo"),
        ];
        for &(input, expected) in cases {
            assert_eq!(
                SourceModelName::new(input)
                    .unwrap()
                    .effective_source_basename()
                    .unwrap(),
                expected
            );
        }
        let plain = SourceModelName::new(b"Foo.m2").unwrap();
        let suffixed_nul = SourceModelName::new(b"Foo.m2.vmo\0").unwrap();
        assert_eq!(
            plain.effective_source_basename().unwrap(),
            suffixed_nul.effective_source_basename().unwrap()
        );
        assert_ne!(plain, suffixed_nul);
        assert_ne!(
            plain.safe_basename().unwrap(),
            suffixed_nul.safe_basename().unwrap()
        );
    }

    #[test]
    fn rejects_empty_reserved_and_embedded_nul_names() {
        for input in [b"".as_slice(), b"\0"] {
            assert_eq!(
                SourceModelName::new(input),
                Err(SourceModelNameError::EmptyBody)
            );
        }
        for input in [b".".as_slice(), b"..", b".\0", b"..\0"] {
            assert_eq!(
                SourceModelName::new(input),
                Err(SourceModelNameError::ReservedBasename)
            );
        }
        for input in [b"a\0b".as_slice(), b"a\0\0", b"a\0b\0"] {
            assert_eq!(
                SourceModelName::new(input),
                Err(SourceModelNameError::EmbeddedNul { offset: 1 })
            );
        }
        assert_eq!(
            SourceModelName::new(b"\0a"),
            Err(SourceModelNameError::EmbeddedNul { offset: 0 })
        );
    }

    #[test]
    fn checks_every_body_byte_with_and_without_terminal_nul() {
        for byte in 0..=u8::MAX {
            for input in [vec![b'a', byte, b'b'], vec![b'a', byte, b'b', 0]] {
                let result = SourceModelName::new(&input);
                match byte {
                    0 => assert_eq!(result, Err(SourceModelNameError::EmbeddedNul { offset: 1 })),
                    b'/' | b'\\' | b':' => assert_eq!(
                        result,
                        Err(SourceModelNameError::PathSeparator { offset: 1, byte })
                    ),
                    b' '..=b'~' => assert_eq!(result.unwrap().as_bytes(), input),
                    _ => assert_eq!(
                        result,
                        Err(SourceModelNameError::NonPrintableByte { offset: 1, byte })
                    ),
                }
            }
        }
        for input in [b"...".as_slice(), b" ", b"~", b"CON", b"nul", b"a?*<>|\""] {
            assert!(SourceModelName::new(input).is_ok(), "{input:?}");
        }
    }

    #[test]
    fn bounds_include_the_optional_terminal_nul() {
        let maximum = [b'x'; 500];
        let name = SourceModelName::new(&maximum).unwrap();
        assert_eq!(name.as_bytes().len(), 500);
        assert_eq!(name.effective_source_basename().unwrap().len(), 504);
        let mut maximum_with_nul = maximum;
        maximum_with_nul[499] = 0;
        let name = SourceModelName::new(&maximum_with_nul).unwrap();
        assert_eq!(name.as_bytes().len(), 500);
        assert_eq!(name.raw_basename().len(), 499);
        assert_eq!(name.effective_source_basename().unwrap().len(), 499);
        let mut oversized = [b'x'; 501];
        for terminal in [b'x', 0] {
            oversized[500] = terminal;
            assert_eq!(
                SourceModelName::new(&oversized),
                Err(SourceModelNameError::InputLimit {
                    bytes: 501,
                    maximum: 500
                })
            );
        }
        assert_eq!(SourceModelName::new(b"a").unwrap().raw_basename(), b"a");
        assert_eq!(SourceModelName::new(b"a\0").unwrap().raw_basename(), b"a");
    }

    #[test]
    fn digest_matches_independent_python_hashlib_vectors() {
        // hashlib.sha256(b"benilla-source-model-name-v1\0" +
        //     struct.pack("<I", len(name)) + name).hexdigest()
        let cases: &[(&[u8], &str)] = &[
            (
                b"Foo.m2",
                "417770e95ab73722be0bface075025b79c6f792bbd3daac9de7474b4b2e243e7",
            ),
            (
                b"Foo.m2\0",
                "9d3dbe9dc7a55b4be04ee4c0bff3ccc9bc7627dde3333140e5a2f9658156cde4",
            ),
            (
                b"foo.m2",
                "e26ebe4b84553d8fd7a420cb2c10d2e024b6f244d04c1e355faf2a227e36353e",
            ),
            (
                b"a",
                "ec7b28c6ac6e5271ca3b3b5ed968b26fc2740399376ed9747cb9d520dc43598f",
            ),
            (
                &[b'x'; 500],
                "6d1e6d9f3d6f3afdf9eea9d1728945a08f2319dd5c14c7b9f7922b857e346d7e",
            ),
        ];
        for &(input, digest) in cases {
            let name = SourceModelName::new(input).unwrap();
            assert_eq!(
                name.safe_basename().unwrap(),
                format!("source-{digest}.vmo")
            );
        }
    }

    #[test]
    fn safe_names_have_fixed_length_and_portable_characters() {
        let names: &[&[u8]] = &[
            b"a",
            b"a\0",
            b"CON",
            b"NUL.",
            b"...",
            b" \"*?<>| ",
            &[b'x'; 500],
        ];
        for input in names {
            let safe = SourceModelName::new(input)
                .unwrap()
                .safe_basename()
                .unwrap();
            assert_eq!(safe.len(), 75);
            assert!(safe.starts_with("source-"));
            assert!(safe.ends_with(".vmo"));
            assert!(safe.as_bytes()[7..71]
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)));
            assert!(safe.bytes().all(|byte| byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'.')));
        }
    }
}
