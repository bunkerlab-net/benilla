// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored component fixtures, not an executed ObjectMgr or sealed F1 admission.
use super::*;
use benilla_sim::source_transports::{MoTransport, TaxiNode};

fn animation(id: u32, ordinal: u32, time: u32) -> TransportAnimationRecord {
    TransportAnimationRecord {
        id,
        ordinal,
        entry: 42,
        time,
        position: [0, 0, 0],
    }
}
fn movement() -> MoTransport {
    MoTransport {
        entry: 42,
        path: 1,
        speed: 30,
        acceleration: 1,
    }
}
fn template() -> TransportTemplate {
    let nodes: Vec<_> = (0..7)
        .map(|index| TaxiNode {
            id: index + 1,
            path: 1,
            index,
            map: 0,
            position: [(index as f32 * 100.0).to_bits(), 0, 0],
            flags: if index == 1 || index == 5 { 2 } else { 0 },
            delay: 3,
        })
        .collect();
    generate_template(movement(), Arc::from(nodes), &BTreeMap::from([(0, 0)])).unwrap()
}
fn definitions() -> BTreeMap<u32, TransportGameObject> {
    BTreeMap::from([(
        42,
        TransportGameObject {
            entry: 42,
            kind: 15,
            display: 0,
            movement: movement(),
        },
    )])
}
#[test]
fn animation_uses_effective_id_order_before_same_time_overwrite() {
    let mut templates = SourceTransportTemplates::default();
    let mut events = Vec::new();
    templates
        .load_animations(
            [
                animation(9, 0, 50),
                animation(2, 1, 50),
                animation(9, 2, 60),
                animation(2, 3, 60),
            ],
            &mut |e| {
                events.push(e);
                Ok(())
            },
        )
        .unwrap();
    let a = templates.animation(42).unwrap().unwrap();
    assert_eq!(a.total_time, 60);
    assert_eq!(a.nodes.len(), 1);
    assert_eq!(a.nodes[&60].id, 9);
    assert!(matches!(
        events.as_slice(),
        [
            TransportTemplateEvent::Animation {
                id: 2,
                time: 60,
                replaced: None,
                ..
            },
            TransportTemplateEvent::Animation {
                id: 9,
                time: 60,
                replaced: Some(2),
                ..
            }
        ]
    ));
}
#[test]
fn empty_animation_store_is_loaded_but_not_a_template_owner() {
    let mut templates = SourceTransportTemplates::default();
    templates.load_animations([], &mut |_| Ok(())).unwrap();
    assert!(templates.animation(42).unwrap().is_none());
    assert_eq!(
        templates.ready().unwrap_err().id,
        "execution.transport.templates_owner"
    );
}
#[test]
fn contradictory_animation_occurrence_refuses_and_poisoned_store_stays_closed() {
    let mut templates = SourceTransportTemplates::default();
    assert_eq!(
        templates
            .load_animations([animation(2, 1, 50), animation(2, 1, 60)], &mut |_| Ok(()))
            .unwrap_err()
            .id,
        "execution.transport.animation_occurrence"
    );
    assert_eq!(
        templates.animation(42).unwrap_err().id,
        "execution.transport.animation_owner"
    );
    assert_eq!(
        templates
            .load_animations([], &mut |_| Ok(()))
            .unwrap_err()
            .id,
        "execution.transport.animation_load_state"
    );
}
#[test]
fn observer_failure_is_not_successful_animation_load() {
    let mut templates = SourceTransportTemplates::default();
    assert_eq!(
        templates
            .load_animations([animation(2, 1, 50)], &mut |_| Err(refuse(
                "authored.observer"
            )))
            .unwrap_err()
            .id,
        "authored.observer"
    );
    assert!(templates.animation(42).is_err());
}
#[test]
fn sql_period_overrides_preserve_vector_duplicates_and_unrelated_fields() {
    let mut templates = SourceTransportTemplates::default();
    let before = template();
    templates.templates.insert(42, before.clone());
    let rows = [
        TransportSqlRow {
            ordinal: 0,
            entry: 42,
            name: b"first".to_vec(),
            period: 12345,
        },
        TransportSqlRow {
            ordinal: 1,
            entry: 42,
            name: b"second".to_vec(),
            period: 0,
        },
    ];
    let mut events = Vec::new();
    templates
        .assign_periods(
            &definitions(),
            &BTreeMap::from([(0, 0)]),
            &rows,
            &[],
            &mut |e| {
                events.push(e);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(templates.by_map[&0], [42, 42]);
    let mut expected = before;
    expected.path_time = 0;
    expected.key_frames.last_mut().unwrap().departure_time = 0;
    assert_eq!(templates.templates[&42], expected);
    assert!(matches!(
        events.as_slice(),
        [
            TransportTemplateEvent::AssignedPeriod { period: 12345, .. },
            TransportTemplateEvent::AssignedPeriod { period: 0, .. }
        ]
    ));
}
#[test]
fn source_row_rejections_and_empty_first_query_preserve_integrity_branch() {
    let mut templates = SourceTransportTemplates::default();
    templates.templates.insert(42, template());
    let mut definitions = definitions();
    definitions.insert(
        43,
        TransportGameObject {
            entry: 43,
            kind: 1,
            display: 0,
            movement: movement(),
        },
    );
    definitions.insert(
        44,
        TransportGameObject {
            entry: 44,
            kind: 15,
            display: 0,
            movement: movement(),
        },
    );
    let rows: Vec<_> = [99, 43, 44, 42]
        .into_iter()
        .enumerate()
        .map(|(n, entry)| TransportSqlRow {
            ordinal: n as u64,
            entry,
            name: vec![],
            period: 1,
        })
        .collect();
    let integrity = [TransportIntegrityRow {
        ordinal: 0,
        guid: 7,
        entry: 42,
        name: b"named".to_vec(),
    }];
    let mut events = Vec::new();
    templates
        .assign_periods(
            &definitions,
            &BTreeMap::new(),
            &rows,
            &integrity,
            &mut |e| {
                events.push(e);
                Ok(())
            },
        )
        .unwrap();
    let reasons: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let TransportTemplateEvent::RejectedRow { reason, .. } = e {
                Some(*reason)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        reasons,
        [
            TransportRowRejection::MissingGameObject,
            TransportRowRejection::WrongType,
            TransportRowRejection::MissingTemplate,
            TransportRowRejection::MissingStartMap
        ]
    );
    assert!(matches!(
        events.last(),
        Some(TransportTemplateEvent::Integrity(_))
    ));
    events.clear();
    templates
        .assign_periods(&definitions, &BTreeMap::new(), &[], &integrity, &mut |e| {
            events.push(e);
            Ok(())
        })
        .unwrap();
    assert!(events.is_empty());
}
#[test]
fn retained_catalog_loads_templates_as_labelled_raw_definition_fixture() {
    let (Ok(root), Ok(occurrences)) = (
        std::env::var("BENILLA_F2_CATALOG"),
        std::env::var("BENILLA_F2_DBC_OCCURRENCES"),
    ) else {
        println!(
            "SKIP retained transport inputs: set BENILLA_F2_CATALOG and BENILLA_F2_DBC_OCCURRENCES"
        );
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let inputs = catalog
        .execution_transport_inputs(Path::new(&occurrences))
        .unwrap();
    struct RawDefinitionFixture(Arc<BTreeMap<u32, TransportGameObject>>);
    impl TransportDefinitionOwner for RawDefinitionFixture {
        fn gameobject_definitions(&mut self) -> Result<Arc<BTreeMap<u32, TransportGameObject>>> {
            Ok(Arc::clone(&self.0))
        }
    }
    let mut fixture = RawDefinitionFixture(Arc::new(inputs.raw_gameobjects));
    let mut templates = SourceTransportTemplates::default();
    let mut events = Vec::new();
    templates
        .load_animations_from_catalog(&mut catalog, &mut |e| {
            events.push(e);
            Ok(())
        })
        .unwrap();
    templates
        .load_templates_from_catalog(
            &mut catalog,
            Path::new(&occurrences),
            &mut fixture,
            &mut |e| {
                events.push(e);
                Ok(())
            },
        )
        .unwrap();
    let t = templates.inventory();
    assert!(t.animations_loaded && t.templates_loaded && !t.stopped);
    assert!(!t.templates.is_empty());
    assert!(events
        .iter()
        .any(|e| matches!(e, TransportTemplateEvent::AssignedPeriod { .. })));
    println!("Unsealed raw-definition transport fixture: {} templates, {} animations, {} period assignments; no effective ObjectMgr or F1 acceptance",
        t.templates.len(),t.animations.len(),events.iter().filter(|e|matches!(e,TransportTemplateEvent::AssignedPeriod{..})).count());
}
