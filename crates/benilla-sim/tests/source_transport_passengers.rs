use benilla_sim::source_transports::{passenger_offset, passenger_position, passenger_tangent};
fn compare(text: &str) -> usize {
    let mut count = 0;
    for line in text.lines() {
        let w = line
            .split_whitespace()
            .map(|s| u32::from_str_radix(s, 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(w.len(), 8);
        let angle = w[0];
        assert_eq!(passenger_tangent(angle).unwrap(), w[1], "tanf {angle:08x}");
        let position = passenger_position(
            [
                7.25_f32.to_bits(),
                (-3.75_f32).to_bits(),
                1.125_f32.to_bits(),
                (-0.25_f32).to_bits(),
            ],
            [
                100.5_f32.to_bits(),
                (-200.25_f32).to_bits(),
                10_f32.to_bits(),
                angle,
            ],
        )
        .unwrap();
        assert_eq!(position[..3], w[2..5], "position {angle:08x}");
        let offset = passenger_offset(
            [
                107.75_f32.to_bits(),
                (-204_f32).to_bits(),
                11.125_f32.to_bits(),
                (-0.25_f32).to_bits(),
            ],
            [
                100.5_f32.to_bits(),
                (-200.25_f32).to_bits(),
                10_f32.to_bits(),
                angle,
            ],
        )
        .unwrap();
        assert_eq!(
            [offset[0], offset[1], offset[3]],
            w[5..8],
            "offset {angle:08x}"
        );
        assert_eq!(offset[2], 1.125_f32.to_bits());
        count += 1;
    }
    count
}
#[test]
fn source_tangent_quotient_matches_pinned_builder_not_inverse_rotation() {
    assert_eq!(
        compare(include_str!("fixtures/source_transport_passengers.tsv")),
        168
    );
}
#[test]
fn retained_dense_orientation_grid_or_clean_absence() {
    let Some(root) = std::env::var_os("BENILLA_F4_PASSENGER_ORACLE") else {
        println!("SKIP retained passenger numeric grid: BENILLA_F4_PASSENGER_ORACLE absent");
        return;
    };
    let text = std::fs::read_to_string(std::path::Path::new(&root).join("expected.tsv")).unwrap();
    assert_eq!(compare(&text), 65568);
}
#[test]
fn numeric_domain_rejects_outside_existing_trig_range() {
    for b in [0x40c9_0fdc, 0xc0c9_0fdc, 0x7f80_0000, 0x7fc0_0000] {
        assert!(passenger_tangent(b).is_err());
    }
    assert!(passenger_tangent(0x40c9_0fdb).is_ok());
    assert!(passenger_tangent(0xc0c9_0fdb).is_ok());
}
