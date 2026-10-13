// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit selection route. The request factory cannot construct or supply a Controller;
//! every production request must pass the owner, native-profile, runtime and catalog gates.
use super::*;
use crossbeam_channel::{Receiver, Sender};

type Prepare = Box<
    dyn FnOnce(
            &Path,
            &ProfileStore,
            &Character,
            benilla_config::Ruleset,
        ) -> Result<Controller, Error>
        + Send,
>;
pub(crate) struct Request {
    prepare: Prepare,
    controls: Receiver<Control>,
}
impl Request {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Only a future supported native compiler can supply this request"
        )
    )]
    pub(crate) fn new(
        entry: Entry,
        data: WorldData,
        rules: Rules,
        catalog: impl ClientCatalog + Send + 'static,
    ) -> (Self, Sender<Control>) {
        let (sender, controls) = crossbeam_channel::bounded(1);
        let prepare = Box::new(
            move |root: &Path, profile: &ProfileStore, character: &Character, rates| {
                Controller::enter(
                    root,
                    profile,
                    character,
                    entry,
                    data,
                    apply_rates(rules, rates)?,
                    &catalog,
                )
            },
        );
        (Self { prepare, controls }, sender)
    }
    pub(crate) fn enter(
        self,
        root: &Path,
        profile: &ProfileStore,
        character: &Character,
        rates: benilla_config::Ruleset,
    ) -> Result<(Controller, Receiver<Control>), Error> {
        Ok((
            (self.prepare)(root, profile, character, rates)?,
            self.controls,
        ))
    }
}
/// The app uses this same route. A requested gameplay failure is the outcome, never a signal
/// to try exploration/realm or Create after Open. Context is moved into exactly one branch.
pub(crate) fn dispatch<C, T, E>(
    requested: Option<Request>,
    context: C,
    gameplay: impl FnOnce(C, Request) -> Result<T, E>,
    exploration: impl FnOnce(C) -> Result<T, E>,
) -> Result<T, E> {
    match requested {
        Some(request) => gameplay(context, request),
        None => exploration(context),
    }
}
