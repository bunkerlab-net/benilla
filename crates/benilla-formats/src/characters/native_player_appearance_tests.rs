//! Raw authored records loaded through the production MPQ/catalog path.
use super::*;
fn source_rows() -> (Vec<[u32; 10]>, Vec<[u32; 9]>) {
    let mut sections = vec![];
    let mut facial = vec![];
    for race in 1..=8 {
        for gender in 0..=1 {
            for ty in 0..=3 {
                sections.push([
                    sections.len() as u32 + 1,
                    race,
                    gender,
                    ty,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ]);
            }
            facial.push([race, gender, 0, 10, 20, 30, 40, 50, 60]);
        }
    }
    (sections, facial)
}
fn load(
    sections: &[[u32; 10]],
    facial: &[[u32; 9]],
    section_stride: usize,
    facial_stride: usize,
) -> Result<CharCreateCatalog> {
    let fixture = Fixture::new();
    let mut members = fixture.members(fixture.race_bytes(116));
    members.iter_mut().find(|m| m.0 == CHAR_SECTIONS).unwrap().1 =
        dbc(sections, b"\0", section_stride);
    members
        .iter_mut()
        .find(|m| m.0 == CHAR_FACIAL_HAIR_STYLES)
        .unwrap()
        .1 = dbc(facial, b"\0", facial_stride);
    let temp = Temp::new();
    temp.write("dbc.MPQ", &members);
    temp.load()
}
#[test]
fn loaded_appearance_tables_preserve_every_record_and_column() {
    let (sections, facial) = source_rows();
    let catalog = load(&sections, &facial, 40, 36).unwrap();
    let rows = catalog.native_appearance_rows().unwrap();
    assert_eq!(rows.sections(), sections);
    assert_eq!(rows.facial_styles(), facial);
}

#[test]
fn raw_rows_preserve_full_width_values_flags_and_duplicates_without_policy() {
    let (mut sections, mut facial) = source_rows();
    sections.push(sections[0]);
    facial.push(facial[0]);
    // Unknown full-width keys are data, not byte-narrowed selectors. The regular
    // baseline rows keep the independent UI catalog complete.
    sections.push([
        u32::MAX,
        0xffff,
        0xfffe,
        0xfffd,
        0xfffc,
        0xfffb,
        0x8000_0000,
        0xffff_fffe,
        0x1234_5678,
        u32::MAX,
    ]);
    facial.push([
        0xffff,
        0xfffe,
        0xfffd,
        u32::MAX,
        0x8000_0000,
        0xffff_fffe,
        0x1234_5678,
        257,
        511,
    ]);
    let catalog = load(&sections, &facial, 40, 36).unwrap();
    let rows = catalog.native_appearance_rows().unwrap();
    assert_eq!(rows.sections(), sections);
    assert_eq!(rows.facial_styles(), facial);
}

#[test]
fn nonexact_record_strides_do_not_expose_complete_raw_tables() {
    let (sections, facial) = source_rows();
    for (section_stride, facial_stride) in [(36, 36), (44, 36), (40, 32), (40, 40)] {
        if let Ok(catalog) = load(&sections, &facial, section_stride, facial_stride) {
            assert!(catalog.native_appearance_rows().is_none());
        }
    }
}

#[test]
fn missing_truncated_or_misdeclared_tables_never_look_complete() {
    let (sections, facial) = source_rows();
    for member_name in [CHAR_SECTIONS, CHAR_FACIAL_HAIR_STYLES] {
        for mutation in 0..6 {
            let fixture = Fixture::new();
            let mut members = fixture.members(fixture.race_bytes(116));
            members.iter_mut().find(|m| m.0 == CHAR_SECTIONS).unwrap().1 =
                dbc(&sections, b"\0", 40);
            members
                .iter_mut()
                .find(|m| m.0 == CHAR_FACIAL_HAIR_STYLES)
                .unwrap()
                .1 = dbc(&facial, b"\0", 36);
            if mutation == 0 {
                members.retain(|m| m.0 != member_name);
            } else {
                let bytes = &mut members.iter_mut().find(|m| m.0 == member_name).unwrap().1;
                match mutation {
                    1 => {
                        bytes.pop();
                    } // declared string block truncated
                    2 => {
                        bytes.truncate(23);
                    } // record body truncated
                    3 => bytes[8..12].copy_from_slice(&99u32.to_le_bytes()),
                    4 => bytes.extend_from_slice(&[0]), // undeclared trailing byte
                    5 => bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes()),
                    _ => unreachable!(),
                }
            }
            let temp = Temp::new();
            temp.write("dbc.MPQ", &members);
            if let Ok(catalog) = temp.load() {
                assert!(
                    catalog.native_appearance_rows().is_none(),
                    "{member_name}, mutation {mutation}"
                );
            }
        }
    }
}
