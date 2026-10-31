//! Where each raise of an event into the VM comes from, checked in debug builds. The reference
//! signals an event only inside the step that causes it (`SignalEvent` `0x703e50`, called from the
//! packet's handler, the input record's dispatch, the verb's body or the frame pass's walk), so a
//! raise here is right inside a script call (a verb segment, a handler, the host's apply of a
//! call), a lane's firing or the VM's own tick. One made anywhere else is a system raising at its
//! own place in the frame, which the source scans cannot see when it goes through a helper in
//! another module: it fails, naming its caller, unless its scoped identifier is on the host's
//! allow list with its reason. Each raise is judged where it enters the VM, at one of its two
//! doors: the fire (`tick::fire_event_into`) or the push onto the host queue (`Model::queue_event`).
//! A queued one fires later inside the tick, and a held one inside the host's drain.
//!
//! The check runs only while the host has armed it ([`arm`]), around its frame's schedules, so a
//! test that drives a VM by hand is not judged. How an unlisted raise fails is the host's
//! [`Failure`]: a test panics, and a live run reports each site once and goes on. Without
//! `debug_assertions` every piece compiles to nothing.

/// A place a raise is right to come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A script call: a verb segment's fan-out, a handler running, the host applying a call or
    /// running what waited for it.
    Script,
    /// A lane's firing, the host's place for what a packet raised.
    Lane,
    /// The VM's own tick: the queued events, the `OnUpdate` walk and the pump.
    Tick,
    /// A host function's explicit identifier, checked against the armed allow list.
    Allowed(&'static str),
}

/// An explicit host origin that may raise, with why it may.
#[derive(Clone, Copy, Debug)]
pub struct Allow {
    /// The identifier passed to [`Origin::Allowed`], normally the function's module path.
    pub id: &'static str,
    /// Why it raises at its own place: a reference fact, or the guard row that names it.
    pub why: &'static str,
}

/// How a raise from an unlisted origin fails.
#[derive(Clone, Copy, Debug)]
pub enum Failure {
    /// A panic naming the event and its caller, for a test.
    Panic,
    /// The same line handed to the host's reporter, once per site and scoped origin, for a live
    /// run that must go on.
    Report(fn(&str)),
}

/// What the host armed: its allow list and how a raise outside it fails.
type ArmedState = Option<(&'static [Allow], Failure)>;

#[cfg(debug_assertions)]
mod imp {
    use std::cell::{Cell, RefCell};
    use std::collections::HashSet;

    use super::{ArmedState, Failure, Origin};

    thread_local! {
        /// The innermost origin open on this thread.
        static OPEN: Cell<Option<Origin>> = const { Cell::new(None) };
        /// The host's allow list and failure while the check is armed.
        static ARMED: Cell<ArmedState> = const { Cell::new(None) };
        /// The sites reported so far ([`Failure::Report`]).
        static REPORTED: RefCell<Option<HashSet<String>>> = const { RefCell::new(None) };
    }

    pub(super) fn set_current(origin: Option<Origin>) -> Option<Origin> {
        OPEN.replace(origin)
    }

    pub(super) fn current() -> Option<Origin> {
        OPEN.get()
    }

    pub(super) fn set_armed(armed: ArmedState) -> ArmedState {
        ARMED.replace(armed)
    }

    #[track_caller]
    pub(super) fn check(event: &str, lua: Option<&mlua::Lua>) {
        let Some((allow, failure)) = ARMED.get() else {
            return;
        };
        if match current() {
            Some(Origin::Script | Origin::Lane | Origin::Tick) => true,
            Some(Origin::Allowed(id)) => allow.iter().any(|a| a.id == id),
            None => false,
        } {
            return;
        }
        // A Rust function a script called: what it raises is the script call's.
        if lua.is_some_and(|lua| lua.inspect_stack(0, |_| ()).is_some()) {
            return;
        }
        let at = std::panic::Location::caller();
        let from = current();
        let line = format!(
            "`{event}` was raised outside every script call, lane firing and tick, at {at}, \
             from {from:?}: put it on a lane or inside the call that causes it, or give its origin \
             a row on the host's allow list with its reason"
        );
        match failure {
            Failure::Panic => panic!("{line}"),
            Failure::Report(report) => {
                let fresh = REPORTED.with_borrow_mut(|seen| {
                    seen.get_or_insert_with(HashSet::new)
                        .insert(format!("{at} {from:?}"))
                });
                if fresh {
                    report(&line);
                }
            }
        }
    }
}

/// Enter `origin` until the returned guard drops on this thread.
#[must_use = "the origin is open while the guard lives"]
#[inline]
pub fn enter(origin: Origin) -> Entered {
    #[cfg(debug_assertions)]
    let previous = imp::set_current(Some(origin));
    #[cfg(not(debug_assertions))]
    let previous = {
        let _ = origin;
        None
    };
    Entered(previous, std::marker::PhantomData)
}

/// [`enter`]'s thread-bound guard, restoring the previous origin even on panic.
pub struct Entered(Option<Origin>, std::marker::PhantomData<*mut ()>);

impl Drop for Entered {
    #[inline]
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        imp::set_current(self.0);
        #[cfg(not(debug_assertions))]
        let _ = self.0;
    }
}

/// Run `f` as `origin`: a raise inside it is that origin's.
#[inline]
pub fn within<R>(origin: Origin, f: impl FnOnce() -> R) -> R {
    let _origin = enter(origin);
    f()
}

/// The innermost origin open on this thread; `None` without `debug_assertions`.
pub fn current() -> Option<Origin> {
    #[cfg(debug_assertions)]
    {
        imp::current()
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// Arm the check on this thread with `allow` and `failure` until the guard drops.
#[must_use = "the check is armed while the guard lives"]
pub fn arm(allow: &'static [Allow], failure: Failure) -> Armed {
    #[cfg(debug_assertions)]
    {
        Armed(imp::set_armed(Some((allow, failure))))
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (allow, failure);
        Armed(None)
    }
}

/// Arm the check on this thread with `allow` and `failure` until [`disarm`], for a host whose
/// frame opens and closes in two calls, as the host's schedules around its frame do.
pub fn arm_until_disarmed(allow: &'static [Allow], failure: Failure) {
    #[cfg(debug_assertions)]
    imp::set_armed(Some((allow, failure)));
    #[cfg(not(debug_assertions))]
    let _ = (allow, failure);
}

/// End what [`arm_until_disarmed`] began.
pub fn disarm() {
    #[cfg(debug_assertions)]
    imp::set_armed(None);
}

/// [`arm`]'s guard: puts back what was armed before.
pub struct Armed(ArmedState);

impl Drop for Armed {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        imp::set_armed(self.0);
        #[cfg(not(debug_assertions))]
        let _ = self.0;
    }
}

/// Judge a raise of `event` here: `lua` tells whether a script is running when the caller has it.
#[inline]
#[track_caller]
pub(crate) fn check(event: &str, lua: Option<&mlua::Lua>) {
    #[cfg(debug_assertions)]
    imp::check(event, lua);
    #[cfg(not(debug_assertions))]
    let _ = (event, lua);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::tick::{fire_event_into, verb};
    use crate::script::UiScript;

    /// Armed, a verb segment's raise fires, and its queued and fired forms alike pass; outside
    /// every origin the same raise fails, naming the line that made it.
    #[test]
    #[cfg_attr(not(debug_assertions), ignore = "the check compiles out")]
    fn a_raise_inside_a_segment_passes_and_one_outside_every_origin_fails_with_its_caller() {
        let s = UiScript::new().unwrap();
        let _armed = arm(&[], Failure::Panic);
        verb(&s.lua, |model| model.raise("CURSOR_UPDATE", Vec::new()));
        within(Origin::Lane, || {
            fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new())
        });
        let line = line!() + 2;
        let outside = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
        }));
        let message = outside.expect_err("an unarmed origin raised");
        let message = message
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(
            message.contains("`PLAYER_MONEY`") && message.contains(&format!("origin.rs:{line}:")),
            "{message}"
        );
    }

    thread_local! {
        static REPORTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn keep(line: &str) {
        REPORTS.with_borrow_mut(|r| r.push(line.to_string()));
    }

    /// Armed for a live run, a raise outside every origin goes on, and each site is reported once
    /// however often it raises.
    #[test]
    #[cfg_attr(not(debug_assertions), ignore = "the check compiles out")]
    fn a_live_run_reports_each_site_once_and_goes_on() {
        let s = UiScript::new().unwrap();
        let _armed = arm(&[], Failure::Report(keep));
        for _ in 0..3 {
            fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
        }
        fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
        let reports = REPORTS.with_borrow(Clone::clone);
        assert_eq!(reports.len(), 2, "{reports:#?}");
        assert!(
            reports.iter().all(|r| r.contains("`PLAYER_MONEY`")),
            "{reports:#?}"
        );
    }

    /// Unarmed, as a VM driven by hand is, nothing is judged.
    #[test]
    #[cfg_attr(not(debug_assertions), ignore = "the check compiles out")]
    fn an_unarmed_thread_judges_nothing() {
        let mut s = UiScript::new().unwrap();
        s.fire_event("PLAYER_MONEY", Vec::new());
        s.queue_event("PLAYER_MONEY", Vec::new());
    }

    /// A host identifier grants permission only while its allow-listed scope is open.
    #[test]
    #[cfg_attr(not(debug_assertions), ignore = "the check compiles out")]
    fn an_allowed_identifier_passes_only_in_its_scope() {
        const ALLOW: &[Allow] = &[Allow {
            id: "origin::tests::allowed",
            why: "the test's own",
        }];
        let s = UiScript::new().unwrap();
        let _armed = arm(ALLOW, Failure::Panic);
        within(Origin::Allowed(ALLOW[0].id), || {
            fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
            within(Origin::Tick, || {
                fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
            });
            assert_eq!(current(), Some(Origin::Allowed(ALLOW[0].id)));
        });
        assert_eq!(current(), None);
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
        }))
        .is_err());
    }

    /// An identifier that is not exactly on the allow list grants no permission.
    #[test]
    #[cfg_attr(not(debug_assertions), ignore = "the check compiles out")]
    fn an_unlisted_identifier_is_rejected_and_restored_after_panic() {
        const ALLOW: &[Allow] = &[Allow {
            id: "origin::tests::allowed",
            why: "the test's own",
        }];
        let s = UiScript::new().unwrap();
        let _armed = arm(ALLOW, Failure::Panic);
        for id in [
            "tests::allowed",
            "origin::tests::allowed::closure",
            "unlisted",
        ] {
            within(Origin::Lane, || {
                let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    within(Origin::Allowed(id), || {
                        fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
                    });
                }));
                assert!(failed.is_err(), "{id} passed without an allow entry");
                assert_eq!(current(), Some(Origin::Lane));
                fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
            });
            assert_eq!(current(), None);
        }
    }

    /// Unwinding an allowed scope restores the unmarked state rather than granting later raises.
    #[test]
    #[cfg_attr(not(debug_assertions), ignore = "the check compiles out")]
    fn an_allowed_identifier_restores_after_panic() {
        const ALLOW: &[Allow] = &[Allow {
            id: "origin::tests::allowed",
            why: "the test's own",
        }];
        let s = UiScript::new().unwrap();
        let _armed = arm(ALLOW, Failure::Panic);
        let failed = std::panic::catch_unwind(|| {
            within(Origin::Allowed(ALLOW[0].id), || panic!("unwind the scope"));
        });
        assert!(failed.is_err());
        assert_eq!(current(), None);
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fire_event_into(&s.lua, "PLAYER_MONEY", Vec::new());
        }))
        .is_err());
    }
}
