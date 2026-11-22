// SPDX-License-Identifier: GPL-3.0-or-later
//! Engine-free GridMap AREA and DBCStores flag selection kernels.

#[derive(Debug, Default)]
pub struct AreaData {
    pub global: u16,
    cells: Option<Box<[u16; 256]>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaReadError {
    Truncated,
    Magic,
}
impl AreaData {
    /// GridMap.cpp:155-169 ignores the declared section size and unknown flag bits.
    pub fn decode(bytes: &[u8]) -> Result<Self, AreaReadError> {
        let header = bytes.get(..8).ok_or(AreaReadError::Truncated)?;
        if &header[..4] != b"AREA" {
            return Err(AreaReadError::Magic);
        }
        let flags = u16::from_le_bytes([header[4], header[5]]);
        let global = u16::from_le_bytes([header[6], header[7]]);
        let cells = if flags & 1 == 0 {
            let raw = bytes.get(8..520).ok_or(AreaReadError::Truncated)?;
            let mut cells = Box::new([0; 256]);
            for (cell, pair) in cells.iter_mut().zip(raw.as_chunks::<2>().0) {
                *cell = u16::from_le_bytes(*pair);
            }
            Some(cells)
        } else {
            None
        };
        Ok(Self { global, cells })
    }
    /// GridMap.cpp:137-153 clears the cell allocation, not the stored global flag.
    pub fn unload_cells(&mut self) {
        self.cells = None;
    }
    pub fn get(&self, x: f32, y: f32) -> Option<u16> {
        match &self.cells {
            Some(cells) => Some(cells[cell_index(x, y)?]),
            None => Some(self.global),
        }
    }
}

/// GridMap.cpp:266-270. Reject the float-to-int undefined domain before casting.
pub fn cell_index(x: f32, y: f32) -> Option<usize> {
    fn axis(v: f32) -> Option<usize> {
        let v = 16.0 * (32.0 - v / 533.333_3_f32);
        (v.is_finite() && (-2_147_483_648.0..2_147_483_648.0).contains(&v))
            .then_some(((v as i32) & 15) as usize)
    }
    Some(axis(x)? * 16 + axis(y)?)
}

/// Integer-to-unsigned narrowing is defined modulo 65536, GridMap.cpp:1008.
pub fn narrow_flag(value: u32) -> u16 {
    value as u16
}

/// DBCStores.cpp:676-704. Iterate indexed IDs, retain the last wrong-map match.
pub struct FlagSearch<T> {
    flag: u16,
    map: u32,
    wrong_map: Option<T>,
}
impl<T> FlagSearch<T> {
    pub fn new(flag: u16, map: u32) -> Self {
        Self {
            flag,
            map,
            wrong_map: None,
        }
    }
    pub fn visit(&mut self, entry: T, flag: u32, map: u32) -> Option<T> {
        if self.flag != 0 && flag == u32::from(self.flag) {
            if map == self.map {
                return Some(entry);
            }
            self.wrong_map = Some(entry);
        }
        None
    }
    pub fn wrong_map(self) -> Option<T> {
        self.wrong_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn area_signed_conversion_and_unsigned_narrowing_have_distinct_domains() {
        assert_eq!(cell_index(0.0, 0.0), Some(0));
        assert_eq!(cell_index(f32::NAN, 0.0), None);
        assert_eq!(cell_index(f32::MAX, 0.0), None);
        assert_eq!(narrow_flag(65_535), 65_535);
        assert_eq!(narrow_flag(65_536), 0);
        assert_eq!(narrow_flag(u32::MAX), 65_535);
    }
    #[test]
    fn unloading_area_cells_keeps_the_global_flag() {
        let mut bytes = vec![0; 520];
        bytes[..4].copy_from_slice(b"AREA");
        bytes[6..8].copy_from_slice(&6u16.to_le_bytes());
        let mut area = AreaData::decode(&bytes).unwrap();
        assert_eq!(area.get(0.0, 0.0), Some(0));
        area.unload_cells();
        assert_eq!(area.get(0.0, 0.0), Some(6));
    }
}
