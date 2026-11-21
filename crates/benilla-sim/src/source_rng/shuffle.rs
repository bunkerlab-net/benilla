// SPDX-License-Identifier: GPL-3.0-or-later
// GNU libstdc++ GCC 12.2 translation, copyright FSF 2001-2022;
// upstream GPL-3.0-or-later WITH GCC-exception-3.1; see SOURCE-RNG.md.
use super::{Error, EventKind, NoDraw, Operation, RawGenerator, SourceRandom, TraceEvent};

impl<E: RawGenerator, S: FnMut(TraceEvent)> SourceRandom<'_, E, S> {
    /// Preserve forward swaps and the paired-draw optimization; stl_algo.h:3695-3758.
    pub fn shuffle<T>(&mut self, values: &mut [T]) -> Result<(), Error> {
        let count = values.len() as u64;
        self.operation(Operation::Shuffle, [count, 0, 0], |this| {
            if count < 2 {
                this.no_draw(NoDraw::ShuffleTrivial);
                return Ok(());
            }
            let engine_range = E::MAX - E::MIN;
            if engine_range / count >= count {
                let mut index = 1;
                if count.is_multiple_of(2) {
                    let selected = this.uniform_u64(0, 1)?;
                    this.swap(values, index, selected as usize);
                    index += 1;
                }
                while index < values.len() {
                    let b0 = index as u64 + 1;
                    let b1 = b0 + 1;
                    let combined = this.operation(Operation::ShufflePair, [b0, b1, 0], |this| {
                        this.uniform_u64(0, b0 * b1 - 1)
                    })?;
                    this.swap(values, index, (combined / b1) as usize);
                    index += 1;
                    this.swap(values, index, (combined % b1) as usize);
                    index += 1;
                }
                return Ok(());
            }
            for index in 1..values.len() {
                let selected = this.uniform_u64(0, index as u64)?;
                this.swap(values, index, selected as usize);
            }
            Ok(())
        })
    }

    fn swap<T>(&mut self, values: &mut [T], left: usize, right: usize) {
        values.swap(left, right);
        self.emit(EventKind::Swap {
            span: self.span(),
            left: left as u64,
            right: right as u64,
        });
    }
}
