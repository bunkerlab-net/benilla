// SPDX-License-Identifier: GPL-3.0-or-later
//! Scoped per-test-thread instrumentation of actual preparation entry points.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub height_preparations: usize,
    pub terrain_tiles: usize,
    pub collision_preparations: usize,
}
thread_local! {
    static ACTIVE: Cell<Option<Counts>> = const { Cell::new(None) };
}

/// An unwind-safe scope isolates measurements from other tests and fixture setup.
pub(crate) fn measure<T>(run: impl FnOnce() -> T) -> (T, Counts) {
    struct Reset(Option<Counts>);
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.set(self.0));
        }
    }
    let previous = ACTIVE.with(|active| active.replace(Some(Counts::default())));
    let reset = Reset(previous);
    let result = run();
    let counts = ACTIVE.with(|active| active.get().expect("measurement is active"));
    drop(reset);
    (result, counts)
}
fn update(increment: impl FnOnce(&mut Counts)) {
    ACTIVE.with(|active| {
        if let Some(mut counts) = active.get() {
            increment(&mut counts);
            active.set(Some(counts));
        }
    });
}
pub(super) fn height() {
    update(|counts| counts.height_preparations += 1);
}
pub(super) fn terrain_tile() {
    update(|counts| counts.terrain_tiles += 1);
}
pub(in super::super) fn collision() {
    update(|counts| counts.collision_preparations += 1);
}
