// SPDX-License-Identifier: GPL-3.0-or-later
//! Reuse the fixed script-name reader and F2.7 report integrity owner.
use super::*;
use crate::native::source_execution::SourceScriptDefinition;

impl VerifiedCatalog {
    pub(crate) fn execution_registry_names(&mut self) -> Result<Vec<Vec<u8>>> {
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        Ok(self
            .execution_script_name_ids(&schema)?
            .into_keys()
            .collect())
    }
    pub(crate) fn execution_registry_registrations(
        &self,
        report: &Path,
    ) -> Result<Vec<SourceScriptDefinition>> {
        let value = self.execution_script_report(report)?;
        registrations(&value["registrations"]["registry"])
    }
}
fn registrations(registry: &Value) -> Result<Vec<SourceScriptDefinition>> {
    let records = registry["records"]
        .as_array()
        .ok_or_else(|| fail("scripts.registry_records_owner"))?;
    check(
        registry["attempts"] == records.len(),
        "scripts.registry_records_owner",
    )?;
    records
        .iter()
        .enumerate()
        .map(|(ordinal, record)| {
            let text = |value: &Value| {
                value
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| fail("scripts.registry_definition_owner"))
            };
            let slots = record["slots"]
                .as_object()
                .ok_or_else(|| fail("scripts.registry_slots_owner"))?;
            check(
                slots.len() == scripts::SLOTS.len(),
                "scripts.registry_slots_owner",
            )?;
            let slots = scripts::SLOTS
                .iter()
                .map(|&slot| {
                    let raw = slots
                        .get(slot)
                        .and_then(Value::as_str)
                        .ok_or_else(|| fail("scripts.registry_slots_owner"))?;
                    let pointer = u64::from_str_radix(raw, 16)
                        .map_err(|_| fail("scripts.registry_slots_owner"))?;
                    Ok((slot.to_owned(), pointer != 0))
                })
                .collect::<Result<_>>()?;
            Ok(SourceScriptDefinition {
                name: text(&record["name"])?.into_bytes(),
                registration: ordinal as u64,
                operation: text(&record["operation"])?,
                source_file: text(&record["source"]["file"])?,
                source_line: u32::try_from(
                    record["source"]["line"]
                        .as_u64()
                        .ok_or_else(|| fail("scripts.registry_definition_owner"))?,
                )
                .map_err(|_| fail("scripts.registry_definition_owner"))?,
                source_function: text(&record["source"]["function"])?,
                slots,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn registry() -> Value {
        let slots: BTreeMap<_, _> = scripts::SLOTS.iter().map(|slot| (*slot, "0")).collect();
        json!({"attempts":1,"records":[{"name":"absent","id":null,"operation":"1",
            "source":{"file":"authored.cpp","line":1,"function":"AddSC_authored"},"slots":slots}]})
    }
    #[test]
    fn script_registry_capture_keeps_absent_names_without_observed_ids() {
        let rows = registrations(&registry()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, b"absent");
        assert_eq!(rows[0].registration, 0);
        assert_eq!(rows[0].slots.len(), 22);
        assert!(!rows[0].slots["GetGameObjectAI"]);
    }
    #[test]
    fn script_registry_capture_refuses_missing_or_unknown_hook_slots() {
        for (removed, inserted) in [("GetAI", "unknown"), ("GetGameObjectAI", "unknown")] {
            let mut value = registry();
            value["records"][0]["slots"]
                .as_object_mut()
                .unwrap()
                .remove(removed);
            value["records"][0]["slots"][inserted] = json!("0");
            assert_eq!(
                registrations(&value).unwrap_err().id,
                "scripts.registry_slots_owner"
            );
        }
    }
}
