// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_calendar::Tm;

#[derive(Default)]
struct Observe(Vec<WorldStartupObservation>);

impl WorldStartupWork for Observe {
    fn clock(&mut self, site: WorldStartupClockSite) -> Result<ClockSample> {
        panic!("honor reads the cached game time, got {site:?}");
    }

    fn observe(&mut self, value: WorldStartupObservation) -> Result<()> {
        self.0.push(value);
        Ok(())
    }
}

const SECONDS: i64 = 1795246040;
const WEDNESDAY: i32 = 3;
const FIELDS: [usize; 13] = [
    PLAYER_FIELD_BYTES2,
    PLAYER_BYTES_3,
    PLAYER_FIELD_SESSION_KILLS,
    PLAYER_FIELD_YESTERDAY_KILLS,
    PLAYER_FIELD_YESTERDAY_CONTRIBUTION,
    PLAYER_FIELD_THIS_WEEK_KILLS,
    PLAYER_FIELD_THIS_WEEK_CONTRIBUTION,
    PLAYER_FIELD_LAST_WEEK_KILLS,
    PLAYER_FIELD_LAST_WEEK_CONTRIBUTION,
    PLAYER_FIELD_LAST_WEEK_RANK,
    PLAYER_FIELD_LIFETIME_HONORABLE_KILLS,
    PLAYER_FIELD_LIFETIME_DISHONORABLE_KILLS,
    PLAYER_FIELD_BYTES,
];

fn detached() -> (SourcePlayer, SourceObjects, SourceWorld) {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let player = super::super::tests::construct(&mut objects, &mut units);
    objects
        .assign_guid(player.object(), 1, SourceGuid::from_raw(1))
        .unwrap();
    (player, objects, SourceWorld::new(12, 200).unwrap())
}

#[test]
fn create_honor_reads_the_three_cached_dates_without_marking_zero_words() {
    let (mut player, objects, world) = detached();
    let mut work = Observe::default();
    let initial_ranks = (player.honor.rank, player.honor.highest);
    let bytes = [player.bytes_3, u32::from_le_bytes(player.field_bytes)];
    player
        .update_honor(
            &objects,
            &world,
            Calendar::new(CalendarOwner::World, SECONDS),
            WEDNESDAY,
            &mut work,
        )
        .unwrap();
    // Northshire-01 A trace lines 68134692-68134694, thread 12 events 15009-15011.
    let local = WorldStartupObservation::LocalTime {
        seconds: SECONDS,
        civil: Tm {
            sec: 20,
            min: 27,
            hour: 7,
            mday: 21,
            mon: 10,
            year: 126,
            wday: 6,
            yday: 324,
            isdst: 0,
        },
    };
    assert_eq!(work.0, [local, local, local]);
    assert_eq!(player.honor.rank_points, Some(0.0));
    assert_eq!((player.honor.rank, player.honor.highest), initial_ranks);
    assert_eq!(player.honor.standing, 0);
    assert_eq!(player.honor.stored, 0.0);
    assert_eq!(player.honor.stored_kills, [0; 2]);
    assert_eq!(player.honor.today, [0; 2]);
    assert_eq!(player.honor.kills, [0; 3]);
    assert_eq!(player.honor.contribution, [0; 3]);
    assert_eq!(player.honor.lifetime, [0; 2]);
    assert_eq!(player.honor.bar, 0);
    assert_eq!(
        [player.bytes_3, u32::from_le_bytes(player.field_bytes)],
        bytes
    );
    let object = objects.get(player.object()).unwrap();
    assert!(FIELDS
        .into_iter()
        .all(|field| !object.field_changed(field).unwrap()));
}

#[test]
fn honor_refresh_marks_changes_and_keeps_stored_counters() {
    let (mut player, objects, world) = detached();
    let today = (126 << 16) | 324;
    let cp = |date, points, kind, state| HonorCp {
        date,
        points,
        kind,
        state,
        is_kill: true,
    };
    player.honor.cp = vec![
        cp(today, 10., HONORABLE, HK_OLD),
        cp(today - 1, 12., HONORABLE, HK_OLD),
        cp(today - 4, 20., HONORABLE, HK_OLD),
        cp(today + 1, 0., DISHONORABLE, 0),
        cp(today, 99., HONORABLE, HK_DELETED),
    ];
    player.honor.stored = 1500.;
    player.honor.stored_kills = [2, 3];
    player
        .update_honor(
            &objects,
            &world,
            Calendar::new(CalendarOwner::World, SECONDS),
            WEDNESDAY,
            &mut Observe::default(),
        )
        .unwrap();
    let honor = &player.honor;
    assert_eq!(honor.today, [1, 0]);
    assert_eq!(honor.kills, [1, 2, 1]);
    assert_eq!(honor.contribution, [12, 22, 20]);
    // Player.cpp:6477-6478,6487-6488,6515-6517 adds three HKs and one DK.
    assert_eq!(honor.lifetime, [3 + 3, 2 + 1]);
    assert_eq!(honor.stored_kills, [2, 3]);
    assert_eq!(honor.cp[3].state, HK_OLD);
    assert_eq!(honor.bar, 63);
    assert_eq!(player.bytes_3 >> 24, 5);
    assert_eq!(player.field_bytes[3], 5);
    let object = objects.get(player.object()).unwrap();
    for field in FIELDS {
        assert_eq!(
            object.field_changed(field).unwrap(),
            field != PLAYER_FIELD_LAST_WEEK_RANK,
            "{field:#x}"
        );
    }
}

#[test]
fn honor_rejects_undefined_rank_bar_casts() {
    let (mut player, objects, world) = detached();
    let calendar = Calendar::new(CalendarOwner::World, SECONDS);
    for (points, error) in [
        (
            f32::NAN,
            "execution.player_save.Formulas_95.honor_rank_cast_undefined",
        ),
        (
            f32::INFINITY,
            "execution.player_save.Player_6544.honor_rank_points_cast_undefined",
        ),
        (
            1.,
            "execution.player_save.Player_6545.honor_bar_cast_undefined",
        ),
        (
            -1.,
            "execution.player_save.Player_6545.honor_bar_cast_undefined",
        ),
    ] {
        player.honor.stored = points;
        assert_eq!(
            player
                .update_honor(
                    &objects,
                    &world,
                    calendar,
                    WEDNESDAY,
                    &mut Observe::default()
                )
                .unwrap_err()
                .id,
            error
        );
        assert!(!objects
            .get(player.object())
            .unwrap()
            .field_changed(PLAYER_FIELD_BYTES2)
            .unwrap());
    }
    player.honor.stored = -1500.;
    player
        .update_honor(
            &objects,
            &world,
            calendar,
            WEDNESDAY,
            &mut Observe::default(),
        )
        .unwrap();
    assert_eq!(player.honor.bar, 193);
}

#[test]
fn honor_refuses_overflow_at_the_reached_contribution_word() {
    let (mut player, objects, world) = detached();
    let today = (126 << 16) | 324;
    let calendar = Calendar::new(CalendarOwner::World, SECONDS);
    for (day, id) in [
        (
            today - 1,
            "execution.player_save.Player_6558.honor_points_cast_undefined",
        ),
        (
            today,
            "execution.player_save.Player_6561.honor_points_cast_undefined",
        ),
        (
            today - 4,
            "execution.player_save.Player_6564.honor_points_cast_undefined",
        ),
    ] {
        player.honor.cp.clear();
        player.honor.cp.push(HonorCp {
            date: day,
            points: 4_294_967_296.0,
            kind: HONORABLE,
            state: HK_OLD,
            is_kill: false,
        });
        let stop = player
            .update_honor(
                &objects,
                &world,
                calendar,
                WEDNESDAY,
                &mut Observe::default(),
            )
            .unwrap_err();
        assert_eq!(stop.id, id);
    }
}
