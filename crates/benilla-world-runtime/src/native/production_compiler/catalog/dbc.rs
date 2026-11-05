// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-layout DBC input views, never a gameplay or SQL spell-store substitute.
use super::*;
use serde_json::json;
use std::{fs::OpenOptions, io::Write, sync::LazyLock};
#[path = "dbc_occurrences.rs"]
mod occurrences;
static POLICY: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/classic_export/catalog_dbc_layouts.json"
    )))
    .expect("DBC layout metadata")
});
type ReadRange<'a> = Box<dyn FnMut(u64, usize) -> Result<Vec<u8>> + 'a>;
/// Checked source fields in physical order with sparse last-occurrence lookup.
pub struct DbcView<'a> {
    read: ReadRange<'a>,
    descriptor: Descriptor,
    layout: Option<Value>,
    format: Vec<u8>,
    chunks: Vec<[u8; 32]>,
    cached: Option<(usize, Vec<u8>)>,
    count: usize,
    fields: usize,
    size: usize,
    end: u64,
    strings: usize,
    offsets: Vec<usize>,
    by_id: BTreeMap<u32, u32>,
    duplicates: usize,
}
impl<'a> DbcView<'a> {
    /// Number of physical records, including duplicate source IDs.
    pub fn record_count(&self) -> usize {
        self.count
    }
    fn new(mut read: ReadRange<'a>, d: Descriptor, layout: Option<Value>) -> Result<Self> {
        validate_descriptor(&d, 67_108_864)?;
        let mut hash = Sha256::new();
        let mut chunks = Vec::new();
        for at in (0..d.bytes).step_by(FRAME) {
            let size = (d.bytes - at).min(FRAME as u64) as usize;
            let raw = read(at, size)?;
            check(raw.len() == size, "catalog.truncated")?;
            hash.update(&raw);
            chunks.push(Sha256::digest(&raw).into());
        }
        check(format!("{:x}", hash.finalize()) == d.sha256, "catalog.hash")?;
        check(d.bytes >= 20, "dbc.byte_limit_or_truncated_header")?;
        let mut this = Self {
            read,
            descriptor: d,
            layout,
            format: Vec::new(),
            chunks,
            cached: None,
            count: 0,
            fields: 0,
            size: 0,
            end: 0,
            strings: 0,
            offsets: Vec::new(),
            by_id: BTreeMap::new(),
            duplicates: 0,
        };
        let raw = this.range(0, 20)?;
        check(&raw[..4] == b"WDBC", "dbc.magic")?;
        this.count = le(&raw, 4, 4)? as usize;
        this.fields = le(&raw, 8, 4)? as usize;
        this.size = le(&raw, 12, 4)? as usize;
        this.strings = le(&raw, 16, 4)? as usize;
        check(this.count <= 100_000, "dbc.record_limit")?;
        check(
            (1..=4096).contains(&this.fields) && (1..=FRAME).contains(&this.size),
            "dbc.layout",
        )?;
        this.end = 20 + this.count as u64 * this.size as u64;
        check(
            add(this.end, this.strings as u64)? == this.descriptor.bytes,
            "dbc.exact_length",
        )?;
        check(
            this.strings > 0
                && this.range(this.end, 1)? == [0]
                && this.range(this.descriptor.bytes - 1, 1)? == [0],
            "dbc.string_block",
        )?;
        if let Some(layout) = &this.layout {
            let fmt = layout["format"]
                .as_str()
                .ok_or_else(|| fail("dbc.layout"))?;
            check(
                this.fields == fmt.len() && this.size as u64 == number(&layout["record_bytes"])?,
                "dbc.layout",
            )?;
            check(
                fmt.len() <= 173
                    && fmt.bytes().all(|c| b"nidxsfbX".contains(&c))
                    && fmt.bytes().filter(|c| b"nd".contains(c)).count() <= 1,
                "dbc.layout",
            )?;
            this.format = fmt.as_bytes().to_vec();
            let mut offset = 0;
            let mut key_offset = None;
            for kind in fmt.bytes() {
                this.offsets.push(offset);
                if b"nd".contains(&kind) {
                    key_offset = Some(offset);
                }
                offset += if b"bX".contains(&kind) { 1 } else { 4 };
            }
            for ordinal in 0..this.count {
                let raw = this.range(20 + ordinal as u64 * this.size as u64, this.size)?;
                for col in 0..this.format.len() {
                    let kind = this.format[col];
                    if b"xX".contains(&kind) {
                        continue;
                    }
                    let at = this.offsets[col];
                    let width = if kind == b'b' { 1 } else { 4 };
                    check(at + width <= raw.len(), "dbc.layout")?;
                    if kind == b's' {
                        this.string_bytes(le(&raw, at, 4)? as usize)?;
                    }
                }
                let key = if let Some(at) = key_offset {
                    le(&raw, at, 4)? as u32
                } else {
                    ordinal as u32
                };
                this.duplicates += usize::from(this.by_id.insert(key, ordinal as u32).is_some());
                check(this.by_id.len() * 160 <= DECODED, "dbc.index_limit")?;
            }
        }
        Ok(this)
    }
    fn range(&mut self, mut at: u64, mut size: usize) -> Result<Vec<u8>> {
        check(
            size <= FRAME && add(at, size as u64)? <= self.descriptor.bytes,
            "catalog.truncated",
        )?;
        let mut out = Vec::with_capacity(size);
        while size > 0 {
            let page = at as usize / FRAME;
            if self.cached.as_ref().is_none_or(|c| c.0 != page) {
                let raw = (self.read)(
                    page as u64 * FRAME as u64,
                    (self.descriptor.bytes - page as u64 * FRAME as u64).min(FRAME as u64) as usize,
                )?;
                let hash: [u8; 32] = Sha256::digest(&raw).into();
                check(hash == self.chunks[page], "catalog.hash")?;
                self.cached = Some((page, raw));
            }
            let raw = &self.cached.as_ref().unwrap().1;
            let start = at as usize % FRAME;
            let take = size.min(raw.len() - start);
            out.extend_from_slice(&raw[start..start + take]);
            at += take as u64;
            size -= take;
        }
        Ok(out)
    }
    fn string_bytes(&mut self, offset: usize) -> Result<Vec<u8>> {
        check(offset < self.strings, "dbc.string_offset")?;
        if offset == 0 {
            return Ok(Vec::new());
        }
        let mut raw = self.range(self.end + offset as u64, (self.strings - offset).min(16385))?;
        let end = raw
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| fail("dbc.string_termination_or_limit"))?;
        raw.truncate(end);
        Ok(raw)
    }
    fn string(&mut self, offset: usize) -> Result<String> {
        Ok(hex(&self.string_bytes(offset)?))
    }
    /// Source fields retain float bits and hexadecimal string bytes.
    pub fn record(&mut self, ordinal: usize) -> Result<Value> {
        check(ordinal < self.count, "dbc.ordinal")?;
        let raw = self.range(20 + ordinal as u64 * self.size as u64, self.size)?;
        let mut key = ordinal as u32;
        let mut values = Vec::new();
        for col in 0..self.format.len() {
            let kind = self.format[col];
            if b"xX".contains(&kind) {
                continue;
            }
            let at = self.offsets[col];
            let width = if kind == b'b' { 1 } else { 4 };
            check(at + width <= raw.len(), "dbc.layout")?;
            let word = le(&raw, at, width)? as u32;
            if b"nd".contains(&kind) {
                key = word;
            }
            if kind != b'd' {
                values.push(json!([
                    col,
                    (kind as char).to_string(),
                    if kind == b's' {
                        json!(self.string(word as usize)?)
                    } else {
                        json!(word)
                    }
                ]));
            }
        }
        Ok(json!({"ordinal":ordinal,"key":key,"values":values}))
    }
    fn record_with_locales(
        &mut self,
        ordinal: usize,
        overlays: &mut [DbcView<'_>],
    ) -> Result<Value> {
        let mut base = self.record(ordinal)?;
        for overlay in overlays {
            check(overlay.count == self.count, "dbc.locale_record_count")?;
            let other = overlay.record(ordinal)?;
            let left = base["values"].as_array_mut().unwrap();
            let right = other["values"].as_array().unwrap();
            check(left.len() == right.len(), "dbc.locale_layout")?;
            for (a, b) in left.iter_mut().zip(right) {
                check(a[0] == b[0] && a[1] == b[1], "dbc.locale_layout")?;
                if a[1] == "s" && a[2] == "" {
                    a[2] = b[2].clone();
                }
            }
        }
        Ok(base)
    }
    /// A missing key is proven against the complete checked physical file.
    pub fn lookup(&mut self, key: u32) -> Result<Option<Value>> {
        self.by_id
            .get(&key)
            .copied()
            .map(|i| self.record(i as usize))
            .transpose()
    }
    fn words(&mut self, ordinal: u32) -> Result<Vec<u32>> {
        let raw = self.range(20 + u64::from(ordinal) * self.size as u64, self.size)?;
        Ok(raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect())
    }
    fn report(&mut self, mut output: Option<&mut std::fs::File>) -> Result<Value> {
        let (mut raw, mut decoded, mut index) = (Sha256::new(), Sha256::new(), Sha256::new());
        for ordinal in 0..self.count {
            raw.update(self.range(20 + ordinal as u64 * self.size as u64, self.size)?);
            if self.layout.is_some() {
                let value = self.record_with_locales(ordinal, &mut [])?;
                json::bounded_len(&value, FRAME - 1, "dbc.record_output_limit")?;
                let mut record = json::canonical(&value);
                record.push(b'\n');
                decoded.update(&record);
                if let Some(handle) = output.as_deref_mut() {
                    handle.write_all(&record).map_err(|_| fail("dbc.output"))?;
                }
            }
        }
        for (key, ordinal) in &self.by_id {
            index.update(key.to_le_bytes());
            index.update(ordinal.to_le_bytes());
        }
        let indexed = self.layout.as_ref().is_some_and(|l| {
            l["format"]
                .as_str()
                .unwrap()
                .bytes()
                .any(|c| b"nd".contains(&c))
        });
        Ok(
            json!({"sha256":self.descriptor.sha256,"bytes":self.descriptor.bytes,"records":self.count,"fields":self.fields,"record_bytes":self.size,
            "physical_sha256":format!("{:x}",raw.finalize()),
            "decoded_sha256":self.layout.as_ref().map(|_|format!("{:x}",decoded.finalize())),
            "index_sha256":self.layout.as_ref().map(|_|format!("{:x}",index.finalize())),
            "unique_keys":self.layout.as_ref().map(|_|self.by_id.len()),"duplicate_keys":self.layout.as_ref().map(|_|self.duplicates),
            "lookup_extent":self.layout.as_ref().map(|_|if indexed { u64::from(self.by_id.last_key_value().map_or(0, |(k,_)|*k))+1 } else {self.count as u64})}),
        )
    }
}
fn open<'a>(
    catalog: &'a mut VerifiedCatalog,
    inventory: &Value,
    name: &str,
    layout: Option<Value>,
) -> Result<DbcView<'a>> {
    let value = inventory
        .get(name)
        .ok_or_else(|| fail("dbc.mandatory_file_missing"))?;
    let d: Descriptor =
        serde_json::from_value(value.clone()).map_err(|_| fail("catalog.descriptor"))?;
    let held = d.clone();
    DbcView::new(
        Box::new(move |at, size| {
            let peak = catalog.cached.as_ref().map_or(0, |c| c.1.len())
                + catalog.pages.len() * PAGE
                + PAGE * 8
                + FRAME * 4;
            check(peak <= catalog.cache_limit, "catalog.cache_limit")?;
            catalog.metrics.cache_peak = catalog.metrics.cache_peak.max(peak);
            catalog
                .store
                .blob_range(&held, at, size, &mut catalog.metrics, catalog.cache_limit)
        }),
        d,
        layout,
    )
}
fn layout(name: &str) -> Option<Value> {
    POLICY["loaders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["file"] == name)
        .cloned()
}
fn require_loaders(inventory: &Value) -> Result<()> {
    for l in POLICY["loaders"].as_array().unwrap() {
        check(
            inventory.get(l["file"].as_str().unwrap()).is_some(),
            "dbc.mandatory_file_missing",
        )?;
    }
    Ok(())
}
impl VerifiedCatalog {
    /// Opens an enabled pinned server layout without accepting arbitrary paths.
    pub fn dbc_view(&mut self, filename: &str) -> Result<DbcView<'_>> {
        check(
            self.manifest["core_revision"] == POLICY["core_revision"],
            "dbc.source_pin",
        )?;
        let spec = layout(filename).ok_or_else(|| fail("dbc.not_server_loader"))?;
        let inventory = bounded_json(&self.object(&descriptor(&self.manifest["dbc_root"])?, 0)?)?;
        open(self, &inventory, filename, Some(spec))
    }
}
fn raw_relation(catalog: &mut VerifiedCatalog, table: &str) -> Result<Value> {
    let mut hash = Sha256::new();
    let mut count = 0;
    catalog.scan(table, 0, 256, |row| {
        hash.update(row.frame);
        count += 1;
        Ok(())
    })?;
    Ok(json!({"rows":count,"raw_sha256":format!("{:x}",hash.finalize())}))
}
fn animation_timeline(animation: &mut DbcView<'_>) -> Result<Value> {
    let mut animations = BTreeMap::<u32, BTreeMap<u32, Value>>::new();
    let ordinals: Vec<_> = animation.by_id.values().copied().collect();
    for ordinal in ordinals {
        let words = animation.words(ordinal)?;
        animations
            .entry(words[1])
            .or_default()
            .insert(words[2], json!({"ordinal":ordinal,"words":words}));
    }
    Ok(json!(animations.into_iter().map(|(entry,nodes)| (entry.to_string(), json!({"total_time":nodes.last_key_value().unwrap().0,"nodes":nodes.values().collect::<Vec<_>>()}))).collect::<BTreeMap<_,_>>()))
}
fn transports(catalog: &mut VerifiedCatalog, inventory: &Value, schema: &Value) -> Result<Value> {
    use super::super::dbc::{Table, Tables};
    use super::super::extensions::transport_paths::coverage;
    let mut tables = Tables::new();
    for name in ["Map", "TaxiPath", "TaxiPathNode"] {
        let file = format!("{name}.dbc");
        let mut view = open(catalog, inventory, &file, layout(&file))?;
        check(
            view.descriptor.bytes <= (FRAME * 8) as u64,
            "dbc.transport_input_limit",
        )?;
        let mut data = Vec::with_capacity(view.descriptor.bytes as usize);
        for at in (0..view.descriptor.bytes).step_by(FRAME) {
            data.extend_from_slice(
                &view.range(at, (view.descriptor.bytes - at).min(FRAME as u64) as usize)?,
            );
        }
        tables.insert(name.into(), Table::parse(name, data)?);
    }
    let mut templates = BTreeMap::new();
    let cols = spawns::columns(schema, "gameobject_template")?;
    let indexes = ["entry", "type", "data0", "data1", "data2", "displayid"]
        .map(|n| spawns::position(&cols, n))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    catalog.scan("gameobject_template", 0, 256, |row| {
        let mut cells = [None; 256];
        spawns::fill(&row, &mut cells)?;
        if spawns::uint(cells[indexes[1]])? == 15 {
            let mut t = serde_json::Map::new();
            for (n, i) in ["entry", "type", "data0", "data1", "data2", "displayid"]
                .iter()
                .zip(&indexes)
            {
                t.insert((*n).into(), json!(spawns::uint(cells[*i])?));
            }
            templates.insert(spawns::uint(cells[indexes[0]])?, Value::Object(t));
        }
        Ok(())
    })?;
    let mut raw = serde_json::Map::new();
    for name in ["transports", "gameobject_template"] {
        raw.insert(name.into(), raw_relation(catalog, name)?);
    }
    let mut animation = open(
        catalog,
        inventory,
        "TransportAnimation.dbc",
        layout("TransportAnimation.dbc"),
    )?;
    let animation_count = animation.count;
    let animation_views = animation_timeline(&mut animation)?;
    drop(animation);
    let mut display = open(
        catalog,
        inventory,
        "GameObjectDisplayInfo.dbc",
        layout("GameObjectDisplayInfo.dbc"),
    )?;
    let mut plans = BTreeMap::new();
    for (entry, t) in &templates {
        let present = display
            .lookup(t["displayid"].as_u64().unwrap() as u32)?
            .is_some();
        plans.insert(entry.to_string(), json!({"template":t,"display_present":present,"paths":[coverage(&json!({"selection":{"map_id":0}}),Some(t),&tables),coverage(&json!({"selection":{"map_id":1}}),Some(t),&tables)]}));
    }
    drop(display);
    let cols = spawns::columns(schema, "transports")?;
    let entry = spawns::position(&cols, "entry")?;
    let name = spawns::position(&cols, "name")?;
    let period = spawns::position(&cols, "period")?;
    let mut rows = Vec::new();
    catalog.scan("transports",0,256,|row| {
        let mut cells = [None;256]; spawns::fill(&row,&mut cells)?;
        let e = spawns::uint(cells[entry])?;
        rows.push(json!({"ordinal":row.ordinal,"entry":e,"name_hex":hex(cells[name].unwrap_or_default()),"period":spawns::uint(cells[period])?,"template_present":templates.contains_key(&e)}));
        check(rows.len()*256 <= DECODED,"dbc.transport_input_limit")?;
        Ok(())
    })?;
    Ok(
        json!({"raw":raw,"rows":rows,"templates":plans,"animations":animation_views,"taxi_paths":tables["TaxiPath"].count,"taxi_nodes":tables["TaxiPathNode"].count,"animation_records":animation_count,
        "source":"TransportMgr.cpp:28-32,44-55,76-139,305-311; Transports.cpp:35-95; DBCStores.cpp:482-508"}),
    )
}
/// Independently checks every retained DBC and complete transport input views.
pub fn diagnose_catalog_dbc(
    root: &Path,
    occurrence_root: &Path,
    output: Option<&Path>,
) -> Result<Value> {
    let mut catalog = VerifiedCatalog::open(root, false)?;
    check(
        catalog.manifest["core_revision"] == POLICY["core_revision"],
        "dbc.source_pin",
    )?;
    let inventory = bounded_json(&catalog.object(&descriptor(&catalog.manifest["dbc_root"])?, 0)?)?;
    let schema = bounded_json(&catalog.object(&descriptor(&catalog.manifest["schema_root"])?, 0)?)?;
    require_loaders(&inventory)?;
    if let Some(path) = output {
        std::fs::create_dir(path).map_err(|_| fail("dbc.output"))?;
    }
    let mut files = BTreeMap::new();
    for (name, d) in inventory
        .as_object()
        .ok_or_else(|| fail("catalog.dbc_inventory"))?
    {
        if d["bytes"] == 0 {
            check(
                POLICY["zero_byte_archive"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(name))
                    && layout(name).is_none(),
                "dbc.byte_limit_or_truncated_header",
            )?;
            files.insert(name.clone(),json!({"sha256":d["sha256"],"bytes":0,"classification":"zero_byte_archive_not_WDBC","records":null}));
            continue;
        }
        let spec = layout(name);
        let mut view = if spec.is_some() {
            catalog.dbc_view(name)?
        } else {
            open(&mut catalog, &inventory, name, None)?
        };
        let mut handle = if let Some(path) = output.filter(|_| spec.is_some()) {
            Some(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path.join(format!("{name}.jsonl")))
                    .map_err(|_| fail("dbc.output"))?,
            )
        } else {
            None
        };
        let mut report = view.report(handle.as_mut())?;
        report["classification"] = json!(if spec.is_some() {
            "server_loader"
        } else {
            "archive_only"
        });
        report["source_treatment"] = POLICY["disabled"].get(name).cloned().unwrap_or_else(|| {
            json!(if spec.is_some() {
                "mandatory"
            } else {
                "no_enabled_LoadDBC_call"
            })
        });
        report["layout"] = spec.unwrap_or(Value::Null);
        files.insert(name.clone(), report);
    }
    let mut transport = transports(&mut catalog, &inventory, &schema)?;
    transport["occurrences"] = occurrences::verify(&mut catalog, &schema, occurrence_root)?;
    let client = super::super::COMPILER_CONTRACT["dbc_layouts"]["Spell"].clone();
    let strings = client["strings"].as_array().unwrap();
    let fmt: String = (0..173)
        .map(|i| {
            if i == 0 {
                'n'
            } else if strings.contains(&json!(i)) {
                's'
            } else {
                'i'
            }
        })
        .collect();
    let spell = open(
        &mut catalog,
        &inventory,
        "Spell.dbc",
        Some(json!({"format":fmt,"record_bytes":client["size"]})),
    )?
    .report(None)?;
    let sql = raw_relation(&mut catalog, "spell_template")?;
    let locale = locale_selection(
        open(
            &mut catalog,
            &inventory,
            "ChrRaces.dbc",
            layout("ChrRaces.dbc"),
        )?
        .lookup(1)?,
        0,
    )?;
    let mut result = json!({"format":"benilla-dbc-catalog-views-v1","catalog_sha256":catalog.manifest["catalog_sha256"],"layout_policy_sha256":json::digest(&*POLICY),"files":files,"transports":transport,
        "spell_identity":{"client_dbc":spell,"server_sql_rows":sql["rows"],"server_sql_sha256":sql["raw_sha256"],"server_store":"spell_template_not_Spell.dbc"},
        "locale_selection":locale,
        "locale_policy":"base_strings_then_first_nonempty_overlay_by_physical_ordinal","locale_overlays":"not_in_retained_catalog","complete":false,
        "blockers":["dbc.reference_execution_missing","dbc.locale_occurrences_missing","dbc.gameplay_not_admitted"]});
    result["view_sha256"] = json!(json::digest(&result));
    catalog.store.check_all()?;
    Ok(result)
}
fn locale_selection(race: Option<Value>, configured: u32) -> Result<Value> {
    let race = race.ok_or_else(|| fail("dbc.locale_human_missing"))?;
    let values = race["values"]
        .as_array()
        .ok_or_else(|| fail("dbc.locale_layout"))?;
    let mut mask = 0u32;
    for i in 0..8 {
        let value = values
            .iter()
            .find(|v| v[0] == 17 + i && v[1] == "s")
            .ok_or_else(|| fail("dbc.locale_layout"))?;
        if value[2] != "" {
            mask |= 1 << i;
        }
    }
    check(mask != 0, "dbc.locale_undetermined")?;
    let effective = if configured != 255 && configured >= 8 {
        0
    } else {
        configured
    };
    let mut default = mask.trailing_zeros();
    if effective < 8 && mask & (1 << effective) != 0 {
        default = effective;
    }
    Ok(
        json!({"configured_locale":configured,"effective_config_locale":effective,
        "available_mask":mask,"default_locale":default,
        "fallback_locales":(0..8).map(|i|if mask & (1 << i) != 0 {i} else {default}).collect::<Vec<_>>(),
        "source":"World.cpp:1462-1504; World.h:556-559"}),
    )
}
#[cfg(test)]
#[path = "dbc_tests.rs"]
mod tests;
