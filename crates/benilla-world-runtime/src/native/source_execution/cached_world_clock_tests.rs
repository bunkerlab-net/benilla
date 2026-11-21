use super::*;
#[test]
fn cached_world_clock_starts_at_epoch_and_unbound_update_refuses() {
    let mut clock = SourceWorldClock::default();
    assert_eq!(clock.current_ms(), 0);
    assert_eq!(
        clock.update().unwrap_err().id,
        "execution.clock.World_1512.recorded_clock_owner"
    );
    assert_eq!(clock.current_ms(), 0);
    let other_map = &clock;
    assert_eq!(other_map.current_ms(), 0);
}
#[test]
fn seal16_first_world_update_follows_the_creation_cast() {
    use std::io::BufRead;
    let Some(path) = std::env::var_os("BENILLA_F5_EVENT_AI_TRACE") else {
        eprintln!("SKIP cached World clock evidence: BENILLA_F5_EVENT_AI_TRACE absent");
        return;
    };
    let mut reader = std::io::BufReader::new(std::fs::File::open(path).unwrap());
    let mut text = String::new();
    let mut line = 0u64;
    let mut tick = None;
    loop {
        text.clear();
        assert_ne!(
            reader.read_line(&mut text).unwrap(),
            0,
            "missing World::Update"
        );
        line += 1;
        let mut fields = text.trim_end().splitn(4, '\t');
        let thread = fields.next().unwrap();
        fields.next().unwrap();
        let kind = fields.next().unwrap();
        let data = fields.next().unwrap_or_default();
        if kind == "world_tick" && tick.is_none() {
            tick = Some((thread.to_owned(), line));
        }
        if kind == "operation_begin" && data.ends_with(" World::Update") {
            assert_eq!((thread, line), ("12", 61753333));
            assert_eq!(
                tick.as_ref().map(|(thread, line)| (thread.as_str(), *line)),
                Some(("12", 61753325))
            );
            assert!(60477770 < line);
            println!("F6_COOLDOWN_CLOCK cached_epoch_ms=0 cast_event_line=60477770 first_world_tick=61753325 first_World_Update=61753333 thread=12 new_clock_samples=0 source=Map.cpp:83-85,World.cpp:108,1512");
            break;
        }
    }
}
