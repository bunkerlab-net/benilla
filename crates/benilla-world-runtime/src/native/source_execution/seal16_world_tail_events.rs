// SPDX-License-Identifier: GPL-3.0-or-later
//! Trace inputs and results for World.cpp:1416-1417.
use super::*;
use std::io::{BufRead, BufReader};

const SELECT: &str = "SELECT event FROM game_event_status";
const ACTIVE: [u16; 4] = [11, 29, 35, 400];
const NEXT_MS: i64 = 1_408_000;

#[derive(Default)]
pub(super) struct EventStartupEvidence {
    announce: Option<bool>,
    select: Vec<u64>,
    writes: Vec<u64>,
    submits: Vec<(u64, u64)>,
    callbacks: Vec<u64>,
    hole_reads: BTreeMap<(u16, u32), u32>,
    subscriber_births: [(u64, u64); 2],
    subscriber_guids: [(u32, u64); 2],
}

impl EventStartupEvidence {
    pub(super) fn observe_subscriber(&mut self, thread: u64, line: u64, kind: &str, p: &[&str]) {
        let (slot, birth) = match line {
            60637267 => (0, true),
            60637306 => (0, false),
            61175059 => (1, true),
            61175093 => (1, false),
            _ => return,
        };
        assert_eq!(thread, 6);
        let address =
            |value: &str| u64::from_str_radix(value.strip_prefix("0x").unwrap(), 16).unwrap();
        if birth {
            assert_eq!((kind, p[1], p[2]), ("logical_birth", "object", "6"));
            assert_eq!(p[3], ["730", "2651"][slot]);
            self.subscriber_births[slot] = (p[0].parse().unwrap(), address(p[4]));
        } else {
            assert_eq!(kind, "object_identity");
            assert_eq!(p[3], "175885");
            let (token, pointer) = self.subscriber_births[slot];
            assert_eq!((p[5].parse().unwrap(), address(p[0])), (token, pointer));
            self.subscriber_guids[slot] = (p[1].parse().unwrap(), pointer);
        }
    }

    pub(super) fn bind_subscribers(&self, maps: &mut ReadySourceContinents) {
        assert_eq!(
            self.subscriber_guids,
            [(15508, 0x200c_01f1_a3f0), (20802, 0x200c_02da_f470)]
        );
        let mut after = None;
        while let Some(id) = maps.next_map(None, after) {
            let map = maps.map_mut(id).unwrap();
            if id.map == 0 {
                for (guid, address) in self.subscriber_guids {
                    let object = map
                        .objects
                        .lookup_database(id, DatabaseLookupKind::GameObject, guid)
                        .unwrap();
                    map.objects
                        .bind_event_notification_address(id, object, address)
                        .unwrap();
                }
            }
            let mut observed = Vec::new();
            map.objects
                .for_each_event_subscriber(id, EventNotificationOrder::SourcePointer, |handle| {
                    observed.push(map.objects.get(handle)?.database_guid);
                    Ok(())
                })
                .unwrap();
            if id.map == 0 {
                assert_eq!(observed, self.subscriber_guids.map(|(guid, _)| guid));
            } else {
                assert!(observed.is_empty());
            }
            after = Some(id);
        }
    }

    pub(super) fn observe(&mut self, line: u64, kind: &str, fields: &[&str]) {
        match line {
            9801 => {
                assert_eq!(
                    (kind, fields),
                    ("config", &["Event.Announce", "file", "0"][..])
                );
                assert!(self.announce.replace(fields[2] != "0").is_none());
            }
            61722964 => {
                assert_eq!(kind, "sql_query_request");
                assert_eq!(fields, ["4294967301", "203340"]);
                self.select.push(line);
            }
            61722965 | 61723082 => {
                assert_eq!(
                    kind,
                    if line == 61722965 {
                        "sql_request_bytes"
                    } else {
                        "sql_query"
                    }
                );
                assert_eq!(unhex(fields[5]), SELECT.as_bytes());
                self.select.push(line);
            }
            61722967 => {
                assert_eq!(
                    (kind, &fields[..3]),
                    ("clock_sample", &["1", "443366", "109701825"][..])
                );
                self.select.push(line);
            }
            61722968 => {
                assert_eq!((kind, fields), ("clock_ms", &["136659"][..]));
                self.select.push(line);
            }
            61722969 => {
                assert_eq!(kind, "sql_clock_request");
                assert_eq!(fields, ["4294967301", "44", "1880", "1"]);
                self.select.push(line);
            }
            61723084 => {
                assert_eq!(kind, "sql_query_result");
                assert_eq!(&fields[..2], ["4294967301", "203340"]);
                assert_eq!(&fields[3..], ["0", "1", "0"]);
                self.select.push(line);
            }
            61723152 | 61723255 | 61723319 | 61724963 => {
                let request = match line {
                    61723152 => "203341",
                    61723255 => "203344",
                    61723319 => "203347",
                    _ => "203350",
                };
                assert_eq!(
                    (kind, fields),
                    ("sql_write_result", &["4294967302", request, "1", "0"][..])
                );
                self.writes.push(line);
            }
            61723094..=61727830 => match kind {
                "message_submit" => {
                    assert_eq!(fields[0], "0x2002001170b0");
                    self.submits.push((line, fields[1].parse().unwrap()));
                }
                "operation_begin" if fields.get(2) == Some(&"OnEventHappened") => {
                    self.callbacks.push(line)
                }
                "choice_empty_event_schedule" => {
                    let id = fields[0].parse().unwrap();
                    let site = fields[1].parse().unwrap();
                    assert_eq!(
                        fields[2],
                        match site {
                            2 => "206761",
                            3 => "206762",
                            _ => panic!("site {site}"),
                        }
                    );
                    let value = fields[3].parse().unwrap();
                    assert_eq!(fields[4], "1");
                    assert!(self.hole_reads.insert((id, site), value).is_none());
                }
                "mt_raw" => panic!("unexpected event startup RNG at :{line}"),
                _ => (),
            },
            _ => (),
        }
    }

    pub(super) fn bind_config(&self, events: &SourceGameEventManager) -> bool {
        assert_eq!(self.writes, [61723152, 61723255, 61723319, 61724963]);
        assert_eq!(
            self.select,
            [61722964, 61722965, 61722967, 61722968, 61722969, 61723082, 61723084]
        );
        // GameEventData() leaves scheduleType unwritten. Bind the two reached reads
        // at GameEventMgr.cpp:676 without making the recorded active list an input.
        let definitions = &events.inventory().definitions;
        let holes = definitions
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, d)| (d.start, d.end, d.length, d.occurrence) == (1, 0, 0, 0));
        let mut expected_reads = 0;
        for (id, definition) in holes {
            for site in [2, 3] {
                assert_eq!(self.hole_reads[&(id as u16, site)], definition.schedule);
                expected_reads += 1;
            }
        }
        assert_eq!(expected_reads, 1914);
        assert_eq!(self.hole_reads.len(), expected_reads);
        self.announce.unwrap()
    }

    pub(super) fn assert_results(
        &self,
        recording: &Path,
        events: &SourceGameEventManager,
        owner: &Owner<'_>,
        status_writes: &[(u64, String)],
    ) {
        assert_eq!(events.inventory().active, BTreeSet::from(ACTIVE));
        assert_eq!(owner.effects.status(), &events.inventory().active);
        assert_eq!(owner.world.event_interval(), NEXT_MS);
        assert!(owner.update.is_empty() && owner.checks.is_empty());
        assert_eq!(
            status_writes
                .iter()
                .map(|(line, _)| *line)
                .collect::<Vec<_>>(),
            [61723133, 61723250, 61723314, 61724958]
        );
        assert_eq!(
            owner.sql.iter().map(String::as_str).collect::<Vec<_>>(),
            std::iter::once(SELECT)
                .chain(status_writes.iter().map(|(_, sql)| sql.as_str()))
                .collect::<Vec<_>>()
        );
        let rows = BufReader::new(
            std::fs::File::open(recording.join("private-db-statement-clocks.tsv")).unwrap(),
        );
        let mut range = rows.lines().skip(369);
        for (idx, sql) in owner.sql.iter().enumerate() {
            let line = range.next().unwrap().unwrap();
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields[0], if idx == 0 { "1880" } else { "1881" });
            assert_eq!(
                fields[1],
                [
                    "1794314193.530385",
                    "1794314193.575346",
                    "1794314193.582205",
                    "1794314193.584786",
                    "1794314193.661990"
                ][idx]
            );
            assert_eq!(unhex(fields[2]), sql.as_bytes());
            assert_eq!(fields[3], if idx == 0 { "0" } else { "1" });
        }
        assert_eq!(
            self.submits,
            [(61723166, 1), (61723193, 2), (61723235, 3), (61723719, 4)]
        );
        let messages = owner.world.messages.inventory();
        assert_eq!(messages.queue.len(), 5);
        assert!(matches!(
            messages.queue[0].payload,
            SourceMessage::SetGraveyardTeam { .. }
        ));
        for (envelope, (_, ordinal)) in messages.queue[1..].iter().zip(&self.submits) {
            assert_eq!(envelope.id, ordinal + 1);
            assert!(matches!(
                envelope.payload,
                SourceMessage::ForwardGameEvent { .. }
            ));
        }
        assert_eq!(
            self.callbacks,
            [61723168, 61723178, 61723266, 61723276, 61723330, 61723344, 61724974, 61724984]
        );
        assert_eq!(
            owner.event_notifications,
            ACTIVE
                .into_iter()
                .flat_map(|id| [(0, id, true, false), (1, id, true, false)])
                .collect::<Vec<_>>()
        );
        let console =
            BufReader::new(std::fs::File::open(recording.join("run/console.log")).unwrap());
        let console: Vec<_> = console
            .lines()
            .skip(615)
            .take(6)
            .map(|line| line.unwrap())
            .collect();
        for (line, id) in console.iter().zip(ACTIVE) {
            assert!(line.starts_with(&format!("GameEvent {id} ")) && line.ends_with(" started."));
        }
        assert_eq!(
            &console[4..],
            [
                "Next game event check in 1408 seconds.",
                "Game Event system initialized."
            ]
        );
    }
}
