//! Complete raw appearance-table records, independent of character eligibility rules.
use super::*;

/// Full-width records obtained from the loaded tables. Construction is private so
/// callers cannot confuse requested values with records read from an archive.
/// Duplicate keys, flags, and all numeric values are preserved without interpretation.
#[derive(Debug)]
pub struct NativeAppearanceRows {
    sections: Vec<[u32; 10]>,
    facial: Vec<[u32; 9]>,
}
/// Exact header/stride/body coverage; never use the UI parser's narrowed keys.
fn rows<const N: usize>(bytes: &[u8]) -> Option<Vec<[u32; N]>> {
    if bytes.get(..4)? != b"WDBC" {
        return None;
    }
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let count = usize::try_from(word(4)?).ok()?;
    if word(8)? != N as u32 || word(12)? != (N * 4) as u32 {
        return None;
    }
    let body = count.checked_mul(N.checked_mul(4)?)?;
    if 20usize
        .checked_add(body)?
        .checked_add(usize::try_from(word(16)?).ok()?)?
        != bytes.len()
    {
        return None;
    }
    let mut output = Vec::with_capacity(count);
    for index in 0..count {
        let mut row = [0; N];
        for (column, field) in row.iter_mut().enumerate() {
            *field = word(20 + index * N * 4 + column * 4)?;
        }
        output.push(row);
    }
    Some(output)
}
impl NativeAppearanceRows {
    pub(super) fn load(chain: &mut Chain) -> Option<Self> {
        Some(Self {
            sections: rows::<10>(&chain.read_file(CHAR_SECTIONS).ok()?)?,
            facial: rows::<9>(&chain.read_file(CHAR_FACIAL_HAIR_STYLES).ok()?)?,
        })
    }

    pub fn sections(&self) -> &[[u32; 10]] {
        &self.sections
    }

    pub fn facial_styles(&self) -> &[[u32; 9]] {
        &self.facial
    }
}

impl CharCreateCatalog {
    /// Complete raw rows when both tables have the exact supported schema.
    /// This does not select an appearance or interpret any record's eligibility.
    pub fn native_appearance_rows(&self) -> Option<&NativeAppearanceRows> {
        self.native_appearance.as_ref()
    }
}
