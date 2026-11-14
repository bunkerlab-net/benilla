use super::*;

const INDICES: [u16; 6] = [0, 1, 2, 2, 1, 3];
const VERTICES: [[f32; 3]; 5] = [
    [-0.0, 0.0, -0.0],
    [2.0, -0.0, 0.0],
    [0.0, 3.0, -0.0],
    [2.0, 3.0, 0.0],
    [17.0, -19.0, 23.0], // Deliberately unreferenced, still serialized and checked.
];

fn set_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn source(indices: &[u16], vertices: &[[f32; 3]]) -> Vec<u8> {
    let mut bytes = vec![0; HEADER_BYTES + 7];
    bytes[..4].copy_from_slice(b"MD20");
    set_word(&mut bytes, 4, 256);
    let index_offset = bytes.len() as u32;
    set_word(&mut bytes, INDICES_HEADER, indices.len() as u32);
    set_word(&mut bytes, INDICES_HEADER + 4, index_offset);
    for &index in indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    let vertex_offset = bytes.len() as u32;
    set_word(&mut bytes, VERTICES_HEADER, vertices.len() as u32);
    set_word(&mut bytes, VERTICES_HEADER + 4, vertex_offset);
    for vertex in vertices {
        for coordinate in vertex {
            bytes.extend_from_slice(&coordinate.to_le_bytes());
        }
    }
    bytes
}

fn raw(bytes: &[u8]) -> Vec<u8> {
    match extract_m2_collision(bytes).unwrap() {
        M2Collision::RawVmap(raw) => raw,
        M2Collision::NoCollision => panic!("authored collision unexpectedly absent"),
    }
}

#[test]
fn raw_framing_zeros_winding_and_unreferenced_vertex_are_exact() {
    let bytes = source(&INDICES, &VERTICES);
    let output = raw(&bytes);
    assert_eq!(output.len(), RAW_FIXED_BYTES + 2 * 6 + 12 * 5);
    assert_eq!(&output[..8], b"VMAPs05\0");
    assert_eq!(word(&output, 8), 5);
    assert_eq!(word(&output, 12), 1);
    assert_eq!(&output[16..56], &[0; 40]);
    assert_eq!(&output[56..60], b"GRP ");
    assert_eq!(word(&output, 60), 8);
    assert_eq!(word(&output, 64), 1);
    assert_eq!(word(&output, 68), 6);
    assert_eq!(&output[72..76], b"INDX");
    assert_eq!(word(&output, 76), 16);
    assert_eq!(word(&output, 80), 6);
    let swapped: Vec<_> = output[84..96]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|raw| u16::from_le_bytes(*raw))
        .collect();
    assert_eq!(swapped, [0, 2, 1, 2, 3, 1]);
    assert_eq!(&output[96..100], b"VERT");
    assert_eq!(word(&output, 100), 64);
    assert_eq!(word(&output, 104), 5);
    let source_vertices = word(&bytes, VERTICES_HEADER + 4) as usize;
    assert_eq!(&output[108..], &bytes[source_vertices..]);
    assert_eq!(bytes, source(&INDICES, &VERTICES));
    assert_eq!(raw(&bytes), output); // Conversion never mutates the source winding.
}

#[test]
fn both_source_coordinate_passes_preserve_all_finite_vertex_bits() {
    let vertices = [
        [0.0, -0.0, 0.0],
        [-0.0, 0.0, -0.0],
        [f32::MAX, f32::MIN, f32::MIN_POSITIVE],
        [f32::from_bits(1), f32::from_bits(0x8000_0001), -0.0],
    ];
    let bytes = source(&[0, 1, 2], &vertices);
    let output = raw(&bytes);
    let source_vertices = word(&bytes, VERTICES_HEADER + 4) as usize;
    assert_eq!(&output[RAW_FIXED_BYTES + 6..], &bytes[source_vertices..]);
}

#[test]
fn every_truncated_prefix_rejects_instead_of_manufacturing_absence() {
    let bytes = source(&INDICES, &VERTICES);
    for length in 0..bytes.len() {
        assert!(extract_m2_collision(&bytes[..length]).is_err(), "{length}");
    }
}

#[test]
fn no_collision_uses_only_the_validated_header_triangle_count() {
    let mut bytes = vec![0xff; HEADER_BYTES];
    bytes[..4].copy_from_slice(b"MD20");
    set_word(&mut bytes, 4, 257);
    set_word(&mut bytes, INDICES_HEADER, 0);
    assert_eq!(extract_m2_collision(&bytes), Ok(M2Collision::NoCollision));
    set_word(&mut bytes, 4, 258);
    assert_eq!(
        extract_m2_collision(&bytes),
        Err(Error::UnsupportedVersion(258))
    );
    assert!(extract_m2_collision(&bytes[..HEADER_BYTES - 1]).is_err());
}

#[test]
fn consumed_arrays_do_not_depend_on_normals_render_arrays_or_authored_bounds() {
    let mut bytes = source(&INDICES, &VERTICES);
    let expected = raw(&bytes);
    // The source only uses the collision indices and vertices, not these fields.
    bytes[8..INDICES_HEADER].fill(0xff);
    bytes[0xfc..HEADER_BYTES].fill(0xff);
    assert_eq!(raw(&bytes), expected);
}

#[test]
fn malformed_triangle_counts_and_indices_reject() {
    for count in [1, 2, 4, 5] {
        let mut bytes = source(&INDICES, &VERTICES);
        set_word(&mut bytes, INDICES_HEADER, count);
        assert_eq!(
            extract_m2_collision(&bytes),
            Err(Error::IncompleteTriangle { index_count: count })
        );
    }
    for (indices, vertices, ordinal, index) in [
        (vec![0, 1, 5], VERTICES.to_vec(), 2, 5),
        (vec![0, 1, u16::MAX], VERTICES.to_vec(), 2, u16::MAX),
        (vec![0, 0, 0], vec![], 0, 0),
    ] {
        assert_eq!(
            extract_m2_collision(&source(&indices, &vertices)),
            Err(Error::IndexOutOfRange {
                ordinal,
                index,
                vertex_count: vertices.len() as u32,
            })
        );
    }
}

#[test]
fn nonfinite_vertices_reject_even_when_unreferenced() {
    for ordinal in 0..VERTICES.len() {
        for axis in 0..3 {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut vertices = VERTICES;
                vertices[ordinal][axis] = value;
                assert_eq!(
                    extract_m2_collision(&source(&INDICES, &vertices)),
                    Err(Error::NonFiniteVertex { ordinal, axis })
                );
            }
        }
    }
}

#[test]
fn array_ranges_are_checked_including_source_offset_overflow() {
    for (header, array) in [
        (INDICES_HEADER, CollisionArray::Indices),
        (VERTICES_HEADER, CollisionArray::Vertices),
    ] {
        for offset in [0, 1, HEADER_BYTES as u32 - 1] {
            let mut bytes = source(&INDICES, &VERTICES);
            set_word(&mut bytes, header + 4, offset);
            assert_eq!(
                extract_m2_collision(&bytes),
                Err(Error::ArrayBeforeHeader { array, offset })
            );
        }
        let mut bytes = source(&INDICES, &VERTICES);
        set_word(&mut bytes, header + 4, u32::MAX);
        assert_eq!(
            extract_m2_collision(&bytes),
            Err(Error::ArrayRangeOverflow(array))
        );
        set_word(&mut bytes, header + 4, MAX_M2_BYTES as u32);
        assert_eq!(
            extract_m2_collision(&bytes),
            Err(Error::TruncatedArray(array))
        );
    }
    assert_eq!(
        array(&[], CollisionArray::Vertices, u32::MAX, 324, 12),
        Err(Error::ArrayRangeOverflow(CollisionArray::Vertices))
    );
    assert_eq!(chunk_bytes(usize::MAX), Err(Error::OutputSizeOverflow));
    assert_eq!(
        chunk_bytes(i32::MAX as usize),
        Err(Error::OutputSizeOverflow)
    );
}

#[test]
fn resource_limits_reject_before_allocation_or_array_access() {
    let bytes = vec![0; MAX_M2_BYTES + 1];
    assert_eq!(
        extract_m2_collision(&bytes),
        Err(Error::InputLimit {
            bytes: MAX_M2_BYTES + 1,
            maximum: MAX_M2_BYTES,
        })
    );
    for (header, array, maximum) in [
        (
            INDICES_HEADER,
            CollisionArray::Indices,
            MAX_BOUNDING_INDICES,
        ),
        (
            VERTICES_HEADER,
            CollisionArray::Vertices,
            MAX_BOUNDING_VERTICES,
        ),
    ] {
        for count in [maximum + 1, u32::MAX] {
            let mut bytes = source(&INDICES, &VERTICES);
            set_word(&mut bytes, header, count);
            assert_eq!(
                extract_m2_collision(&bytes),
                Err(Error::CountLimit {
                    array,
                    count,
                    maximum,
                })
            );
        }
    }
}

#[test]
fn exact_count_limits_and_last_u16_index_are_supported() {
    let mut indices = vec![0; MAX_BOUNDING_INDICES as usize];
    indices[1] = u16::MAX;
    let vertices = vec![[0.0; 3]; MAX_BOUNDING_VERTICES as usize];
    let output = raw(&source(&indices, &vertices));
    assert_eq!(
        output.len(),
        RAW_FIXED_BYTES + indices.len() * 2 + vertices.len() * 12
    );
    assert_eq!(&output[84..90], &[0, 0, 0, 0, 255, 255]);
}

#[test]
fn only_md20_versions_256_and_257_are_accepted() {
    for version in [0, 255, 258, 263, 264, u32::MAX] {
        let mut bytes = source(&INDICES, &VERTICES);
        set_word(&mut bytes, 4, version);
        assert_eq!(
            extract_m2_collision(&bytes),
            Err(Error::UnsupportedVersion(version))
        );
    }
    let mut bytes = source(&INDICES, &VERTICES);
    let expected = raw(&bytes);
    set_word(&mut bytes, 4, 257);
    assert_eq!(raw(&bytes), expected);
    for magic in [*b"MD21", *b"MDLX", [0; 4]] {
        bytes[..4].copy_from_slice(&magic);
        assert_eq!(
            extract_m2_collision(&bytes),
            Err(Error::UnsupportedMagic(magic))
        );
    }
}

#[test]
fn raw_bytes_match_the_independent_pinned_extractor_oracle() {
    const AUTHORED: &[u8] = include_bytes!(
        "../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/authored-m2.m2.test-fixture"
    );
    const AUTHORED_RAW: &[u8] = include_bytes!(
        "../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/authored-m2.raw.vmap"
    );
    const SIGNED_ZERO: &[u8] = include_bytes!(
        "../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/signed-zero-m2.m2.test-fixture"
    );
    const SIGNED_ZERO_RAW: &[u8] = include_bytes!(
        "../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/signed-zero-m2.raw.vmap"
    );
    assert_eq!(word(AUTHORED, 4), 256);
    assert_eq!(word(SIGNED_ZERO, 4), 257);
    assert_eq!(raw(AUTHORED), AUTHORED_RAW);
    assert_eq!(raw(SIGNED_ZERO), SIGNED_ZERO_RAW);
}
