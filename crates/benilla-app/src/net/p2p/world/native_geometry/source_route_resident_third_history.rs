// SPDX-License-Identifier: GPL-3.0-or-later
//! Historical cache-three ACK after source mutation releases obsolete warm owners.
#[test]
fn resident_controller_third_historical_ack_releases_prior_head_after_source_change_without_replay()
{
    super::source_route_resident_controller::three_epoch_probe(
        super::source_route_resident_three_epoch::Mode::SourceChangeBeforeAck,
    );
}
