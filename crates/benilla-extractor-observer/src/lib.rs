//! External native observation. No extractor identity is admitted by this crate.
//! Linux x86-64 is the only supported tracing ABI.

use serde::{Deserialize, Serialize};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod receipt;

/// Exact execve vectors. No environment variable is inherited or reordered.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub attempt_id: String,
    pub stage: String,
    pub executable: Vec<u8>,
    pub executable_sha256: String,
    pub argv: Vec<Vec<u8>>,
    pub env: Vec<Vec<u8>>,
    pub cwd: Vec<u8>,
    pub level: Level,
    pub gates: Vec<Gate>,
    pub max_events: u64,
    pub max_blob_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    KernelWitness,
    Full,
}

/// Fixed native addresses, not symbol names. PIE relocation must be pinned externally.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub site_id: String,
    pub pc: u64,
    pub original_bytes: Vec<u8>,
    pub window: Option<Window>,
}

/// An explicit instruction whitelist, checked before each native step.
/// A missing PC/byte sequence refuses before executing that instruction.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub end_pc: u64,
    pub max_steps: u64,
    pub preserve_fp: bool,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Instruction {
    pub pc: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    InvalidPlan,
    IdentityChanged,
    UnsupportedPlatform,
    System,
    RegisterLoss,
    UnsupportedXsave,
    CompetingDebugState,
    UnexpectedStop,
    UntrackedChild,
    LostEvent,
    UnsupportedSyscall,
    UnsupportedInstruction,
    ShortRead,
    SharedMemoryOwnership,
    StateChanged,
    FloatingPointChanged,
    ResourceCeiling,
    ReceiptIo,
    NonzeroExit,
}

#[derive(Debug, Serialize)]
pub struct Refusal {
    pub kind: RefusalKind,
    pub detail: String,
}

impl Refusal {
    pub(crate) fn new(kind: RefusalKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}
impl std::error::Error for Refusal {}

#[derive(Debug, Serialize)]
pub struct Completion {
    pub processes: u64,
    pub threads: u64,
    pub events: u64,
    pub breakpoints: u64,
    pub steps: u64,
}

/// Creates a new private receipt directory. Existing attempts are never reused.
/// A refused attempt is irreversible and all traced descendants are killed/reaped.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn observe(plan: &Plan, root: &std::path::Path) -> Result<Completion, Refusal> {
    linux::observe(plan, root)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub fn observe(_plan: &Plan, _root: &std::path::Path) -> Result<Completion, Refusal> {
    Err(Refusal::new(
        RefusalKind::UnsupportedPlatform,
        "requires Linux x86-64 ptrace/XSAVE",
    ))
}
