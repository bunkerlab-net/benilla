// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored complete source owners for forced-stop/shared-ID tests only.
//! No private source capture or production native capability.
use super::*;
pub fn profile() -> Profile {
    synthetic_player_melee_fixture::profile_with_options_and_spell(1000, true, None, false, true)
}
pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], synthetic_fixture::player(), 42)
        .unwrap()
}
/// Same actual source owners with ordinary initializer health for chase/rejoin tests.
pub fn surviving_profile() -> Profile {
    use benilla_sim::player_death as d;
    let mut profile = synthetic_player_melee_fixture::source_fireball_profile(1000);
    profile
        .configure_player_death(
            d::DeathConfig {
                map_instanceable: false,
                pve_reclaim_delay_enabled: true,
            },
            d::DeathSupport::checked(
                &d::REQUIRED_ABSENCES.map(|dependency| (dependency, Some(false))),
            )
            .unwrap(),
        )
        .unwrap();
    profile.validate().unwrap();
    profile
}
pub fn surviving_world() -> NativeWorld {
    NativeWorld::synthetic_component_test(
        surviving_profile(),
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap()
}
/// Composition of the existing authored scene and raw-spell spatial producers.
#[derive(Default)]
pub struct CompleteAuthority {
    scene: synthetic_two_owner_fixture::CompleteAuthority,
    spell: synthetic_spell_fixture::SpatialOnly,
}
impl Authority for CompleteAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(
            request.query,
            Query::SourceSpell(_) | Query::SourceLocation(_)
        ) {
            self.spell.resolve(request)
        } else {
            self.scene.resolve(request)
        }
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        if matches!(reply.request.query, Query::SourceSpell(_)) {
            self.spell.observe_derived(reply)
        } else {
            self.scene.observe_derived(reply)
        }
    }
}
