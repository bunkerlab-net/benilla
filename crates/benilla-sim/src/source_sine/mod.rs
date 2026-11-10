// SPDX-License-Identifier: GPL-3.0-or-later
//! glibc 2.36 __sin_fma binary64 results, with software fused arithmetic.
mod core;
mod table;

use crate::source_moon::{self as moon, Anomaly, CallOwner, Sine64, SineOrder};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const LIBM_SHA256: &str = "7f2ca87f652f56b094462474b076749e90e689d0ecb9cb63c7679820b271b4e7";
pub const FMA_OFFSET: &str = "0x702c0";

/// Component computation only. Historical execution additionally requires DispatchBinding.
pub fn sine_bits(operand: u64) -> u64 {
    core::sine(operand)
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AcquisitionBinding {
    #[serde(rename = "REFERENCE_MANIFEST_SHA256")]
    pub manifest: String,
    #[serde(rename = "REFERENCE_SCHEMA_SHA256")]
    pub schema: String,
    #[serde(rename = "REFERENCE_INSTRUMENTATION_SHA256")]
    pub instrumentation: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ResolvedFunction {
    pub function: String,
    pub module_sha256: String,
    pub relative_offset: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProcessReceipt {
    pub format: String,
    pub source_process_receipt_sha256: String,
    pub source_acquisition_bindings: AcquisitionBinding,
    pub cpu_flags: Vec<String>,
    pub platform: String,
    pub loader_environment: BTreeMap<String, Option<String>>,
    pub resolved_functions_before_gameplay: Vec<ResolvedFunction>,
    pub observed_inside_actual_source_process: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchBinding {
    _private: (),
}
impl DispatchBinding {
    /// The caller authenticates this receipt against its acquisition before projecting it here.
    pub fn from_process_receipt(
        round: u32,
        acquisition: &AcquisitionBinding,
        receipt: &ProcessReceipt,
    ) -> moon::Result<Self> {
        let hash = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        };
        let hashes = [
            &acquisition.manifest,
            &acquisition.schema,
            &acquisition.instrumentation,
        ];
        let mut functions = receipt
            .resolved_functions_before_gameplay
            .iter()
            .filter(|f| f.function == "sin");
        let sine = functions.next();
        if round < 14
            || receipt.format != "benilla-reference-actual-process-environment-v2"
            || !receipt.observed_inside_actual_source_process
            || receipt.platform != "x86_64"
            || !hash(&receipt.source_process_receipt_sha256)
            || !hashes.iter().all(|s| hash(s))
            || &receipt.source_acquisition_bindings != acquisition
            || !["fma", "avx2"]
                .iter()
                .all(|name| receipt.cpu_flags.iter().any(|f| f == name))
            || receipt.loader_environment.get("GLIBC_TUNABLES") != Some(&None)
            || receipt.loader_environment.get("LD_HWCAP_MASK") != Some(&None)
            || functions.next().is_some()
            || !sine
                .is_some_and(|s| s.module_sha256 == LIBM_SHA256 && s.relative_offset == FMA_OFFSET)
        {
            return Err(moon::Error::MissingSineBinding);
        }
        Ok(Self { _private: () })
    }
}

pub struct ReferenceSine {
    owner: CallOwner,
    order: SineOrder,
}
impl ReferenceSine {
    pub fn new(owner: CallOwner, order: SineOrder, _: DispatchBinding) -> Self {
        Self { owner, order }
    }
}
impl Sine64 for ReferenceSine {
    fn owner(&self) -> moon::Result<CallOwner> {
        Ok(self.owner)
    }
    fn order(&self) -> moon::Result<SineOrder> {
        Ok(self.order)
    }
    fn evaluate(&mut self, _: i32, _: Anomaly, operand: u64) -> moon::Result<u64> {
        Ok(sine_bits(operand))
    }
    fn finish(&self) -> moon::Result<()> {
        Ok(())
    }
}
