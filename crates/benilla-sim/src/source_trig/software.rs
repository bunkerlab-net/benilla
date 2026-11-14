// SPDX-License-Identifier: GPL-3.0-or-later
//! Software RNE at each binary64 operation and binary32 conversion; no FMA.
use super::core::Arithmetic;
use crate::facing::soft::{D, S};

pub(super) struct Software;
impl Arithmetic for Software {
    type Double = D;
    fn binary64(bits: u64) -> D {
        D::bits(bits)
    }
    fn binary32(bits: u32) -> D {
        S::bits(bits).double()
    }
    fn integer(value: i32) -> D {
        D::integer(value)
    }
    fn single(value: D) -> u32 {
        value.single().raw()
    }
    fn truncate(value: D) -> Option<i32> {
        value.truncate_i32()
    }
}
