// SPDX-License-Identifier: GPL-3.0-or-later
//! Captured selected model set, combined with map framing in memory only.
use crate::{DoodadProjectionProfile, MapProducts, PreparedDiscovery};
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    fmt,
};
const MAX_MODEL_BYTES: usize = 256 * 1024 * 1024;
const MAX_MODEL_CAPACITY: usize = 512 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;
const MAX_MODEL_POLLS: usize = 131_072;
#[derive(Debug)]
pub enum MapBundleError {
    Cancelled,
    Limit,
    Allocation,
    Name,
    MissingModel,
    AmbiguousOutput,
    Map(crate::MapProductError),
    Model(benilla_native_vmap::ModelWriteError),
    Source(crate::SourceError),
}
impl fmt::Display for MapBundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "vmap.bundle_cancelled",
            Self::Limit => "vmap.bundle_limit",
            Self::Allocation => "vmap.bundle_allocation",
            Self::Name => "vmap.bundle_name",
            Self::MissingModel => "vmap.bundle_missing_model",
            Self::AmbiguousOutput => "vmap.bundle_ambiguous_output",
            Self::Map(_) => "vmap.bundle_map",
            Self::Model(_) => "vmap.bundle_model",
            Self::Source(_) => "vmap.bundle_source",
        })
    }
}
impl std::error::Error for MapBundleError {}
pub struct AssembledModelProduct {
    name: Vec<u8>,
    effective: Vec<u8>,
    raw_product: usize,
    bytes: Vec<u8>,
}
impl AssembledModelProduct {
    /// Exact source set/record identity. May contain one trailing NUL.
    pub fn source_name(&self) -> &[u8] {
        &self.name
    }
    /// Source's effective basename, computed without calling a filesystem API.
    /// This is metadata, not a validated publication path or consumer ProductId.
    pub fn effective_source_basename(&self) -> &[u8] {
        &self.effective
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Exact typed name for a source-aware inventory; construction grants no admission.
    pub fn source_identity(&self) -> Result<benilla_native_vmap::SourceModelName, MapBundleError> {
        benilla_native_vmap::SourceModelName::new(&self.name).map_err(|error| match error {
            benilla_native_vmap::SourceModelNameError::AllocationFailed => {
                MapBundleError::Allocation
            }
            _ => MapBundleError::Name,
        })
    }
}
pub struct AssembledSourceMap<'a> {
    source: DiscoveryStorage<'a>,
    map: MapProducts,
    models: Vec<AssembledModelProduct>,
}
// Both forms retain the exact capture; ownership changes no source-check policy.
#[cfg_attr(
    any(test, feature = "synthetic-map-tests"),
    allow(
        clippy::large_enum_variant,
        reason = "test-only ownership retains the exact capture without an extra allocation"
    )
)]
enum DiscoveryStorage<'a> {
    Borrowed(&'a PreparedDiscovery),
    #[cfg(any(test, feature = "synthetic-map-tests"))]
    Owned(PreparedDiscovery),
}
impl DiscoveryStorage<'_> {
    fn get(&self) -> &PreparedDiscovery {
        match self {
            Self::Borrowed(source) => source,
            #[cfg(any(test, feature = "synthetic-map-tests"))]
            Self::Owned(source) => source,
        }
    }
}
impl fmt::Debug for AssembledSourceMap<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssembledSourceMap")
            .field("map", &self.map.map_id())
            .field("models", &self.models.len())
            .finish_non_exhaustive()
    }
}
impl AssembledSourceMap<'_> {
    pub fn source_receipts(&self) -> impl Iterator<Item = crate::SourceReceipt<'_>> {
        self.source.get().source_receipts()
    }
    pub fn source_policy(&self) -> &'static str {
        self.source.get().source_policy()
    }

    pub fn map(&self) -> &MapProducts {
        &self.map
    }
    pub fn models(&self) -> &[AssembledModelProduct] {
        &self.models
    }
    pub fn verify_unchanged(&self, cancelled: impl Fn() -> bool) -> Result<(), MapBundleError> {
        #[cfg(feature = "synthetic-map-tests")]
        let _runtime = crate::profiling::RuntimeScope::enter();
        self.source
            .get()
            .source
            .verify_unchanged(&cancelled)
            .map_err(MapBundleError::Source)?;
        check(&cancelled)
    }
}
impl PreparedDiscovery {
    /// Consume the exact discovery into immutable products for scoped test readers.
    /// This changes ownership only, not freshness, assembly limits or admission.
    #[cfg(any(test, feature = "synthetic-map-tests"))]
    pub fn into_map_bundle(
        self,
        profile: DoodadProjectionProfile,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<AssembledSourceMap<'static>>, MapBundleError> {
        let Some(AssembledSourceMap { map, models, .. }) =
            self.assemble_map_bundle(profile, cancelled)?
        else {
            return Ok(None);
        };
        Ok(Some(AssembledSourceMap {
            source: DiscoveryStorage::Owned(self),
            map,
            models,
        }))
    }
    pub fn assemble_map_bundle(
        &self,
        profile: DoodadProjectionProfile,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<AssembledSourceMap<'_>>, MapBundleError> {
        self.assemble_map_bundle_with_limits(profile, cancelled, MAX_MODEL_BYTES, MAX_MODEL_POLLS)
    }
    pub(crate) fn assemble_map_bundle_with_limits(
        &self,
        profile: DoodadProjectionProfile,
        cancelled: impl Fn() -> bool,
        model_byte_limit: usize,
        model_poll_limit: usize,
    ) -> Result<Option<AssembledSourceMap<'_>>, MapBundleError> {
        let model_byte_limit = model_byte_limit.min(MAX_MODEL_BYTES);
        let model_poll_limit = model_poll_limit.min(MAX_MODEL_POLLS);
        let Some(map) = self
            .map_products(profile, &cancelled)
            .map_err(MapBundleError::Map)?
        else {
            return Ok(None);
        };
        let names =
            crate::selected_model_names::ordered_unique(map.spawn_model_names(), &cancelled)
                .map_err(name_error)?;
        let mut models = select_models(names, &self.products, &cancelled)?;
        compile_models(
            &mut models,
            &self.products,
            &cancelled,
            model_byte_limit,
            model_poll_limit,
        )?;
        self.source
            .verify_unchanged(&cancelled)
            .map_err(MapBundleError::Source)?;
        check(&cancelled)?;
        Ok(Some(AssembledSourceMap {
            source: DiscoveryStorage::Borrowed(self),
            map,
            models,
        }))
    }
}
fn compile_models(
    models: &mut [AssembledModelProduct],
    products: &[crate::RawProduct],
    cancelled: &impl Fn() -> bool,
    model_byte_limit: usize,
    model_poll_limit: usize,
) -> Result<(), MapBundleError> {
    let polls = Cell::new(0usize);
    let mut bytes = 0usize;
    let mut capacity = 0usize;
    let model_cancelled = || {
        polls.set(polls.get().saturating_add(1));
        polls.get() > model_poll_limit || cancelled()
    };
    for model in models {
        if model_cancelled() {
            return Err(if polls.get() > model_poll_limit {
                MapBundleError::Limit
            } else {
                MapBundleError::Cancelled
            });
        }
        let raw = products[model.raw_product]
            .raw_vmap()
            .ok_or(MapBundleError::MissingModel)?;
        let result = benilla_native_vmap::assemble_raw_model(raw, model_cancelled);
        if polls.get() > model_poll_limit {
            return Err(MapBundleError::Limit);
        }
        model.bytes = result.map_err(|e| {
            if e == benilla_native_vmap::ModelWriteError::Cancelled {
                MapBundleError::Cancelled
            } else {
                MapBundleError::Model(e)
            }
        })?;
        capacity = capacity
            .checked_add(model.bytes.capacity())
            .filter(|n| *n <= MAX_MODEL_CAPACITY)
            .ok_or(MapBundleError::Limit)?;
        bytes = bytes
            .checked_add(model.bytes.len())
            .filter(|n| *n <= model_byte_limit)
            .ok_or(MapBundleError::Limit)?;
    }
    Ok(())
}
fn select_models(
    names: Vec<&[u8]>,
    products: &[crate::RawProduct],
    cancelled: &impl Fn() -> bool,
) -> Result<Vec<AssembledModelProduct>, MapBundleError> {
    let mut raw_lookup = HashMap::new();
    raw_lookup
        .try_reserve(products.len())
        .map_err(|_| MapBundleError::Allocation)?;
    for (i, product) in products.iter().enumerate() {
        check(&cancelled)?;
        if raw_lookup
            .insert(product.extracted_name.as_bytes(), i)
            .is_some()
        {
            return Err(MapBundleError::AmbiguousOutput);
        }
    }
    let mut metadata = 0usize;
    let mut models = Vec::new();
    models
        .try_reserve_exact(names.len())
        .map_err(|_| MapBundleError::Allocation)?;
    for name in names {
        check(&cancelled)?;
        let (prefix, effective) = effective_name(name)?;
        let raw_product = *raw_lookup.get(prefix).ok_or(MapBundleError::MissingModel)?;
        if products[raw_product].raw_vmap().is_none() {
            return Err(MapBundleError::MissingModel);
        }
        let owned_name = copy(name, &mut metadata)?;
        let effective = copy(&effective, &mut metadata)?;
        models.push(AssembledModelProduct {
            name: owned_name,
            effective,
            raw_product,
            bytes: Vec::new(),
        });
    }
    let mut aliases = HashSet::new();
    aliases
        .try_reserve(models.len())
        .map_err(|_| MapBundleError::Allocation)?;
    for model in &models {
        check(&cancelled)?;
        if !aliases.insert(model.effective.as_slice()) {
            return Err(MapBundleError::AmbiguousOutput);
        }
    }
    drop(aliases);
    drop(raw_lookup);
    Ok(models)
}
fn check(cancelled: &impl Fn() -> bool) -> Result<(), MapBundleError> {
    if cancelled() {
        Err(MapBundleError::Cancelled)
    } else {
        Ok(())
    }
}
fn name_error(error: crate::selected_model_names::SelectedModelNamesError) -> MapBundleError {
    use crate::selected_model_names::SelectedModelNamesError as E;
    match error {
        E::Cancelled => MapBundleError::Cancelled,
        E::Limit => MapBundleError::Limit,
        E::Allocation => MapBundleError::Allocation,
        E::Name => MapBundleError::Name,
    }
}
fn copy(bytes: &[u8], total: &mut usize) -> Result<Vec<u8>, MapBundleError> {
    *total = total
        .checked_add(bytes.len())
        .filter(|n| *n <= MAX_METADATA_BYTES)
        .ok_or(MapBundleError::Limit)?;
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len())
        .map_err(|_| MapBundleError::Allocation)?;
    out.extend_from_slice(bytes);
    Ok(out)
}
fn effective_name(name: &[u8]) -> Result<(&[u8], Vec<u8>), MapBundleError> {
    let prefix = name.strip_suffix(&[0]).unwrap_or(name);
    if prefix.is_empty()
        || prefix.len() > 500
        || prefix
            .iter()
            .any(|b| !matches!(b, 32..=126) || matches!(b, b'/' | b'\\' | b':'))
        || matches!(prefix, b"." | b"..")
    {
        return Err(MapBundleError::Name);
    }
    let mut out = Vec::new();
    let extra = if prefix.len() == name.len() { 4 } else { 0 };
    out.try_reserve_exact(prefix.len() + extra)
        .map_err(|_| MapBundleError::Allocation)?;
    out.extend_from_slice(prefix);
    if extra != 0 {
        out.extend_from_slice(b".vmo");
    }
    Ok((prefix, out))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_preserve_source_effective_identity_and_reject_paths() {
        for (name, prefix, effective) in [
            (
                b"Foo.m2".as_slice(),
                b"Foo.m2".as_slice(),
                b"Foo.m2.vmo".as_slice(),
            ),
            (
                b"Foo.m2\0".as_slice(),
                b"Foo.m2".as_slice(),
                b"Foo.m2".as_slice(),
            ),
        ] {
            let (p, e) = effective_name(name).unwrap();
            assert_eq!(p, prefix);
            assert_eq!(e, effective);
        }
        for name in [
            b"".as_slice(),
            b"a\0b",
            b"a\0\0",
            b"..",
            b"a/b",
            b"a\\b",
            b"a:b",
            b"\x80",
        ] {
            assert!(matches!(effective_name(name), Err(MapBundleError::Name)));
        }
    }
    fn raw(name: &str) -> crate::RawProduct {
        crate::RawProduct {
            kind: crate::ModelKind::M2,
            member: name.to_owned(),
            extracted_name: name.to_owned(),
            collision: crate::Collision::M2(vec![0xff]),
            doodads: None,
        }
    }
    #[test]
    fn selected_only_missing_and_ambiguous_models_are_checked_before_conversion() {
        let products = vec![raw("Unused.m2"), raw("Foo.m2"), raw("Foo.m2.vmo")];
        let models = select_models(vec![b"Foo.m2"], &products, &|| false).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].raw_product, 1);
        assert!(models[0].bytes.is_empty());
        assert!(matches!(
            select_models(vec![b"Absent.m2"], &products, &|| false),
            Err(MapBundleError::MissingModel)
        ));
        assert!(matches!(
            select_models(vec![b"Foo.m2", b"Foo.m2.vmo\0"], &products, &|| false),
            Err(MapBundleError::AmbiguousOutput)
        ));
        let models = select_models(vec![b"Foo.m2", b"Foo.m2\0"], &products, &|| false).unwrap();
        assert_eq!(models.len(), 2);
        assert_ne!(models[0].effective, models[1].effective);
        assert_eq!(models[0].raw_product, models[1].raw_product);
        assert!(matches!(
            select_models(vec![b"Foo.m2"], &products, &|| true),
            Err(MapBundleError::Cancelled)
        ));
    }
    #[test]
    fn conversion_ignores_unused_corrupt_raw_and_cancels_in_later_models() {
        const RAW: &[u8] = include_bytes!(
            "../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/authored-m2.raw.vmap"
        );
        let mut products = vec![raw("Unused.m2"), raw("Foo.m2")];
        products[1].collision = crate::Collision::M2(RAW.to_vec());
        let make_models =
            || select_models(vec![b"Foo.m2", b"Foo.m2\0"], &products, &|| false).unwrap();
        let mut models = make_models();
        let polls = Cell::new(0);
        compile_models(
            &mut models,
            &products,
            &|| {
                polls.set(polls.get() + 1);
                false
            },
            MAX_MODEL_BYTES,
            MAX_MODEL_POLLS,
        )
        .unwrap();
        assert_eq!(models[0].bytes, models[1].bytes);
        assert!(!models[0].bytes.is_empty());
        for cutoff in 0..polls.get() {
            let mut models = make_models();
            let count = Cell::new(0);
            let error = compile_models(
                &mut models,
                &products,
                &|| {
                    let n = count.get();
                    count.set(n + 1);
                    n >= cutoff
                },
                MAX_MODEL_BYTES,
                MAX_MODEL_POLLS,
            )
            .unwrap_err();
            assert!(
                matches!(error, MapBundleError::Cancelled),
                "{cutoff}: {error:?}"
            );
        }
    }
}
