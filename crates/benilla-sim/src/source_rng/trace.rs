// SPDX-License-Identifier: GPL-3.0-or-later
//! Caller-owned engine cursors and nested semantic records, without trace allocation.
use super::{Error, Mt19937};

/// Supply source generator bounds and raw words; uniform_int_dist.h:284-295.
pub trait RawGenerator {
    const MIN: u64;
    const MAX: u64;
    fn next_word(&mut self) -> u64;
}

impl RawGenerator for Mt19937 {
    const MIN: u64 = 0;
    const MAX: u64 = u32::MAX as u64;
    fn next_word(&mut self) -> u64 {
        u64::from(self.next_u32())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawCursor {
    pub raw: u64,
    pub next_span: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Raw,
    UniformU32,
    UniformI32,
    UniformU64,
    UniformI64,
    CanonicalF32,
    CanonicalF64,
    UniformF32,
    UniformF64,
    Irand,
    Urand,
    IrandDefault,
    UrandDefault,
    Frand,
    RandNorm,
    RandNormF,
    RandChance,
    RandChanceF,
    RollChanceF,
    RollChanceI,
    ChanceU,
    RollChanceCombat,
    DieRoll,
    PickRandomValue,
    Shuffle,
    ShufflePair,
    WseRandom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    U64(u64),
    I64(i64),
    F32(u32),
    F64(u64),
    Bool(bool),
    Unit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Lemire32,
    Lemire64,
    Fallback,
    Upscale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoDraw {
    CombatRoundedZero,
    SuppliedDie,
    ShuffleTrivial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Begin {
        span: u64,
        parent: Option<u64>,
        at: u64,
        operation: Operation,
        args: [u64; 3],
    },
    Raw {
        span: u64,
        position: u64,
        word: u64,
    },
    Rejected {
        span: u64,
        begin: u64,
        end: u64,
        reason: Rejection,
    },
    NoDraw {
        span: u64,
        reason: NoDraw,
    },
    Swap {
        span: u64,
        left: u64,
        right: u64,
    },
    DieSide {
        span: u64,
        side: u8,
        weight: u32,
        cumulative: u32,
    },
    End {
        span: u64,
        begin: u64,
        end: u64,
        result: Result<Value, Error>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceEvent {
    pub stream: u64,
    pub kind: EventKind,
}

/// Borrow one engine and its continuing cursor; source ownership remains in Util.cpp:35-39.
pub struct SourceRandom<'a, E, S> {
    pub(super) engine: &'a mut E,
    cursor: &'a mut DrawCursor,
    sink: &'a mut S,
    stream: u64,
    parent: Option<u64>,
}

pub(super) trait Recorded: Copy {
    fn recorded(self) -> Value;
}
macro_rules! recorded_integer {
    ($type:ty, $variant:ident, $wide:ty) => {
        impl Recorded for $type {
            fn recorded(self) -> Value {
                Value::$variant(self as $wide)
            }
        }
    };
}
recorded_integer!(u8, U64, u64);
recorded_integer!(u32, U64, u64);
recorded_integer!(u64, U64, u64);
recorded_integer!(i32, I64, i64);
recorded_integer!(i64, I64, i64);
impl Recorded for f32 {
    fn recorded(self) -> Value {
        Value::F32(self.to_bits())
    }
}
impl Recorded for f64 {
    fn recorded(self) -> Value {
        Value::F64(self.to_bits())
    }
}
impl Recorded for bool {
    fn recorded(self) -> Value {
        Value::Bool(self)
    }
}
impl Recorded for () {
    fn recorded(self) -> Value {
        Value::Unit
    }
}

impl<'a, E: RawGenerator, S: FnMut(TraceEvent)> SourceRandom<'a, E, S> {
    pub fn new(
        stream: u64,
        engine: &'a mut E,
        cursor: &'a mut DrawCursor,
        sink: &'a mut S,
    ) -> Result<Self, Error> {
        if E::MIN >= E::MAX {
            return Err(Error::InvalidGenerator);
        }
        Ok(Self {
            engine,
            cursor,
            sink,
            stream,
            parent: None,
        })
    }
    /// Logical source thread owning this continuing engine and cursor.
    pub fn stream(&self) -> u64 {
        self.stream
    }

    pub fn cursor(&self) -> DrawCursor {
        *self.cursor
    }

    pub(super) fn emit(&mut self, kind: EventKind) {
        (self.sink)(TraceEvent {
            stream: self.stream,
            kind,
        });
    }

    pub(super) fn span(&self) -> u64 {
        self.parent.expect("raw work has a semantic span")
    }

    pub(super) fn operation<T: Recorded>(
        &mut self,
        operation: Operation,
        args: [u64; 3],
        body: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let span = self.cursor.next_span;
        self.cursor.next_span = span.checked_add(1).ok_or(Error::CursorOverflow)?;
        let begin = self.cursor.raw;
        let parent = self.parent.replace(span);
        self.emit(EventKind::Begin {
            span,
            parent,
            at: begin,
            operation,
            args,
        });
        let result = body(self);
        self.emit(EventKind::End {
            span,
            begin,
            end: self.cursor.raw,
            result: result.map(Recorded::recorded),
        });
        self.parent = parent;
        result
    }

    pub(super) fn raw(&mut self) -> Result<u64, Error> {
        let position = self.cursor.raw;
        let next = position.checked_add(1).ok_or(Error::CursorOverflow)?;
        let word = self.engine.next_word();
        self.cursor.raw = next;
        self.emit(EventKind::Raw {
            span: self.span(),
            position,
            word,
        });
        if !(E::MIN..=E::MAX).contains(&word) {
            return Err(Error::InvalidGeneratorWord);
        }
        Ok(word)
    }

    pub(super) fn rejected(&mut self, begin: u64, reason: Rejection) {
        self.emit(EventKind::Rejected {
            span: self.span(),
            begin,
            end: self.cursor.raw,
            reason,
        });
    }

    pub(super) fn no_draw(&mut self, reason: NoDraw) {
        self.emit(EventKind::NoDraw {
            span: self.span(),
            reason,
        });
    }

    pub fn next_raw(&mut self) -> Result<u64, Error> {
        self.operation(Operation::Raw, [0; 3], Self::raw)
    }
}
