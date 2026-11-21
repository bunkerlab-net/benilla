// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::CatalogDbcView;

#[derive(Default)]
pub(super) struct Evidence {
    request: bool,
    result: Option<String>,
    rows: Vec<[u32; 9]>,
    ended: bool,
}

impl Evidence {
    pub(super) fn observe(&mut self, line: u64, kind: &str, fields: &[&str]) {
        match line {
            61721736 => {
                assert_eq!(kind, "sql_request_bytes");
                assert_eq!(
                    unhex(fields.last().unwrap()),
                    BATTLEGROUND_TEMPLATE_SQL.as_bytes()
                );
                self.request = true;
            }
            61721743 => {
                assert_eq!(kind, "sql_query_result");
                assert_eq!(&fields[3..], ["3", "9", "0"]);
                self.result = Some(fields[2].to_owned());
            }
            61721794..=61721888 if kind == "sql_field" => {
                assert_eq!(Some(fields[0]), self.result.as_deref());
                let column: usize = fields[1].parse().unwrap();
                let bytes = unhex(fields.last().unwrap());
                let value = std::str::from_utf8(&bytes).unwrap();
                if column == 0 {
                    self.rows.push([0; 9]);
                }
                self.rows.last_mut().unwrap()[column] = if column == 7 {
                    (value.parse::<f64>().unwrap() as f32).to_bits()
                } else {
                    value.parse().unwrap()
                };
            }
            61721907 => {
                assert_eq!(kind, "sql_result_end");
                assert_eq!(fields, ["4294967670"]);
                self.ended = true;
            }
            _ => {}
        }
    }

    pub(super) fn inputs<'a>(
        &self,
        catalog: &'a mut VerifiedCatalog,
        config: &[u8],
        safe_locations: &'a BTreeMap<u32, SourceWorldSafeLoc>,
    ) -> Inputs<'a> {
        assert!(self.request && self.result.is_some() && self.ended);
        let rows = catalog.execution_battleground_rows().unwrap();
        assert_eq!(rows.len(), 3);
        for (row, recorded) in rows.iter().zip(&self.rows) {
            assert_eq!(
                [
                    row.type_id,
                    row.min_per_team,
                    row.max_per_team,
                    row.min_level,
                    row.max_level,
                    row.alliance_start,
                    row.horde_start,
                    row.max_start_distance.to_bits(),
                    row.player_skin_refloot,
                ],
                *recorded
            );
        }
        assert_eq!(self.rows.len(), rows.len());
        assert!(rows.iter().all(|row| row.type_id < 4));
        let (line, locale) = spawn_loader_config::assignment(config, "DBC.Locale").unwrap();
        assert_eq!(line, 814);
        let default_locale = catalog
            .execution_default_dbc_locale(locale.parse().unwrap())
            .unwrap();
        Inputs {
            rows,
            safe_locations,
            maps: catalog.dbc_view("Map.dbc").unwrap(),
            default_locale,
        }
    }
}

pub(super) struct Inputs<'a> {
    rows: Vec<BattleGroundRow>,
    safe_locations: &'a BTreeMap<u32, SourceWorldSafeLoc>,
    maps: CatalogDbcView<'a>,
    default_locale: u8,
}

impl BattleGroundInputs for Inputs<'_> {
    fn templates(&mut self, sql: &str) -> Result<Vec<BattleGroundRow>> {
        assert_eq!(sql, BATTLEGROUND_TEMPLATE_SQL);
        Ok(std::mem::take(&mut self.rows))
    }

    fn safe_locations(&self) -> Option<&BTreeMap<u32, SourceWorldSafeLoc>> {
        Some(self.safe_locations)
    }

    fn map_name(&mut self, map: u32) -> Result<Option<Box<[u8]>>> {
        Ok(self
            .maps
            .execution_localized_map_name(map, self.default_locale)?)
    }

    fn notice(&mut self, notice: BattleGroundLoadNotice) {
        panic!("unexpected battleground load notice: {notice:?}");
    }
}
