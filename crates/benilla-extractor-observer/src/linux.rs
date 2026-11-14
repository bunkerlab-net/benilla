use crate::receipt::{bytes, hash, Receipts};
use crate::{Completion, Gate, Level, Plan, Refusal, RefusalKind};
use libc::{c_void, pid_t, user_regs_struct};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs;
use std::mem::{offset_of, size_of};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const XSTATE: usize = 0x202;
const OPTIONS: libc::c_long = (libc::PTRACE_O_EXITKILL
    | libc::PTRACE_O_TRACESYSGOOD
    | libc::PTRACE_O_TRACEFORK
    | libc::PTRACE_O_TRACEVFORK
    | libc::PTRACE_O_TRACECLONE
    | libc::PTRACE_O_TRACEEXEC
    | libc::PTRACE_O_TRACEEXIT) as libc::c_long;
const MAX_TRANSFER: usize = 16 * 1024 * 1024;

fn refusal(kind: RefusalKind, detail: impl Into<String>) -> Refusal {
    Refusal::new(kind, detail)
}
fn system(operation: &str) -> Refusal {
    refusal(
        RefusalKind::System,
        format!("{operation}: {}", std::io::Error::last_os_error()),
    )
}
fn ptrace(
    request: libc::c_uint,
    tid: pid_t,
    address: usize,
    data: usize,
) -> Result<libc::c_long, Refusal> {
    // All callers pass kernel-checked native ABI pointers or debug-register offsets.
    let result = unsafe { libc::ptrace(request, tid, address as *mut c_void, data as *mut c_void) };
    if result == -1 {
        Err(system("ptrace"))
    } else {
        Ok(result)
    }
}
fn wait(tid: pid_t) -> Result<(pid_t, i32), Refusal> {
    let mut status = 0;
    loop {
        let result = unsafe { libc::waitpid(tid, &mut status, libc::__WALL) };
        if result > 0 {
            return Ok((result, status));
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return Err(system("waitpid"));
        }
    }
}
fn register_set(tid: pid_t, kind: usize, data: &mut [u8]) -> Result<usize, Refusal> {
    let mut vector = libc::iovec {
        iov_base: data.as_mut_ptr().cast(),
        iov_len: data.len(),
    };
    ptrace(
        libc::PTRACE_GETREGSET,
        tid,
        kind,
        (&mut vector as *mut libc::iovec) as usize,
    )?;
    Ok(vector.iov_len)
}
struct State {
    regs: user_regs_struct,
    general: Vec<u8>,
    xsave: Vec<u8>,
}
fn snapshot(tid: pid_t) -> Result<State, Refusal> {
    let mut regs = std::mem::MaybeUninit::<user_regs_struct>::zeroed();
    let general = unsafe {
        std::slice::from_raw_parts_mut(
            regs.as_mut_ptr().cast::<u8>(),
            size_of::<user_regs_struct>(),
        )
    };
    let length = register_set(tid, 1, general)?;
    if length != general.len() {
        return Err(refusal(RefusalKind::RegisterLoss, "short NT_PRSTATUS"));
    }
    let general = general.to_vec();
    let mut xsave = vec![0; 65536];
    let length = register_set(tid, XSTATE, &mut xsave)?;
    if !(576..65536).contains(&length) {
        return Err(refusal(
            RefusalKind::UnsupportedXsave,
            format!("XSAVE size {length}"),
        ));
    }
    xsave.truncate(length);
    // Linux exports standard format, not compacted XSAVE. Unknown future layouts refuse.
    if u64::from_le_bytes(xsave[520..528].try_into().unwrap()) != 0 {
        return Err(refusal(RefusalKind::UnsupportedXsave, "compacted XSAVE"));
    }
    let bv = u64::from_le_bytes(xsave[512..520].try_into().unwrap());
    let cpu = std::arch::x86_64::__cpuid_count(13, 0);
    let supported = cpu.eax as u64 | ((cpu.edx as u64) << 32);
    if bv & !supported != 0 {
        return Err(refusal(
            RefusalKind::UnsupportedXsave,
            "unknown XSTATE_BV component",
        ));
    }
    for bit in 2..64 {
        if bv & (1u64 << bit) != 0 {
            let leaf = std::arch::x86_64::__cpuid_count(13, bit);
            if leaf.eax == 0 || leaf.ebx as usize + leaf.eax as usize > length {
                return Err(refusal(
                    RefusalKind::UnsupportedXsave,
                    format!("missing XSAVE component {bit}"),
                ));
            }
        }
    }
    Ok(State {
        regs: unsafe { regs.assume_init() },
        general,
        xsave,
    })
}
fn unchanged(tid: pid_t, state: &State) -> Result<(), Refusal> {
    let after = snapshot(tid)?;
    if state.general != after.general || state.xsave != after.xsave {
        return Err(refusal(
            RefusalKind::StateChanged,
            format!("saved state changed while stopped: {tid}"),
        ));
    }
    Ok(())
}
fn memory(tid: pid_t, address: u64, length: usize) -> Result<Vec<u8>, Refusal> {
    if length > MAX_TRANSFER {
        return Err(refusal(
            RefusalKind::ResourceCeiling,
            "transfer exceeds 16 MiB",
        ));
    }
    if length == 0 {
        return Ok(Vec::new());
    }
    let mut data = vec![0; length];
    let local = libc::iovec {
        iov_base: data.as_mut_ptr().cast(),
        iov_len: length,
    };
    let remote = libc::iovec {
        iov_base: address as usize as *mut c_void,
        iov_len: length,
    };
    let count = unsafe { libc::process_vm_readv(tid, &local, 1, &remote, 1, 0) };
    if count != length as isize {
        return Err(refusal(
            RefusalKind::ShortRead,
            format!("tid={tid} address={address:#x} requested={length} got={count}"),
        ));
    }
    Ok(data)
}
fn c_string(tid: pid_t, address: u64) -> Result<Vec<u8>, Refusal> {
    let mut data = Vec::new();
    for offset in 0..4096 {
        let byte = memory(tid, address + offset, 1)?[0];
        data.push(byte);
        if byte == 0 {
            return Ok(data);
        }
    }
    Err(refusal(
        RefusalKind::ShortRead,
        "unterminated pathname exceeds 4096 bytes",
    ))
}
fn debug(tid: pid_t, register: usize) -> Result<u64, Refusal> {
    ptrace(
        libc::PTRACE_PEEKUSER,
        tid,
        offset_of!(libc::user, u_debugreg) + register * 8,
        0,
    )
    .map(|value| value as u64)
}
fn set_debug(tid: pid_t, register: usize, value: u64) -> Result<(), Refusal> {
    ptrace(
        libc::PTRACE_POKEUSER,
        tid,
        offset_of!(libc::user, u_debugreg) + register * 8,
        value as usize,
    )
    .map(|_| ())
}
fn arm(tid: pid_t, gates: &[Gate], inherited: bool) -> Result<(), Refusal> {
    let old = debug(tid, 7)?;
    let expected = gates
        .iter()
        .enumerate()
        .fold(0, |bits, (i, _)| bits | (1u64 << (2 * i)));
    if old & 0xffff_00ff != 0 && !(inherited && old & 0xffff_00ff == expected) {
        return Err(refusal(
            RefusalKind::CompetingDebugState,
            format!("DR7={old:#x}"),
        ));
    }
    set_debug(tid, 7, 0)?;
    for index in 0..4 {
        set_debug(tid, index, gates.get(index).map_or(0, |gate| gate.pc))?;
    }
    set_debug(tid, 6, 0)?;
    set_debug(tid, 7, expected)?;
    if debug(tid, 7)? & 0xffff_00ff != expected {
        return Err(refusal(
            RefusalKind::CompetingDebugState,
            "DR7 readback mismatch",
        ));
    }
    Ok(())
}

#[derive(Default)]
struct Thread {
    process: u64,
    logical: u64,
    seq: u64,
    newborn: bool,
    creation_event: u64,
    exit_seen: bool,
    deliver_signal: i32,
    birth_return_pending: bool,
    pending: Option<Syscall>,
    window: Option<ActiveWindow>,
}
struct ActiveWindow {
    gate: usize,
    steps: u64,
    fp_before: Vec<u8>,
    parent_event: u64,
    instruction: NativeCapture,
}
struct NativeCapture {
    pc: u64,
    code: Vec<u8>,
    address: Option<u64>,
    before: Vec<u8>,
}
struct Syscall {
    number: u64,
    args: [u64; 6],
    payload: Value,
    entry_event: u64,
}

#[derive(Clone, Default)]
struct Descriptors {
    next: u64,
    live: BTreeMap<u64, u64>,
}
impl Descriptors {
    fn open(&mut self, fd: u64) -> u64 {
        self.next += 1;
        self.live.insert(fd, self.next);
        self.next
    }
    fn initial(tid: pid_t, receipts: &mut Receipts<'_>) -> Result<Self, Refusal> {
        let mut result = Self::default();
        let entries =
            fs::read_dir(format!("/proc/{tid}/fd")).map_err(|_| system("fd inventory"))?;
        let mut entries = entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| system("fd inventory entry"))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let fd = entry
                .file_name()
                .to_string_lossy()
                .parse::<u64>()
                .map_err(|_| refusal(RefusalKind::LostEvent, "non-numeric fd"))?;
            let target = fs::read_link(entry.path()).map_err(|_| system("fd target"))?;
            let generation = result.open(fd);
            receipts.event(1,0,0,None,None,"inherited_descriptor","observed",json!({"raw_fd":fd,"fd_generation":generation,"target":bytes(target.as_os_str().as_bytes())}))?;
        }
        Ok(result)
    }
}
struct Tracees {
    tids: BTreeMap<pid_t, Thread>,
}
impl Drop for Tracees {
    fn drop(&mut self) {
        // EXITKILL covers observer death. Explicit termination also covers errors/receipt failure.
        for tid in self.tids.keys() {
            unsafe {
                libc::kill(*tid, libc::SIGKILL);
            }
        }
        while !self.tids.is_empty() {
            match wait(-1) {
                Ok((tid, status)) if libc::WIFEXITED(status) || libc::WIFSIGNALED(status) => {
                    self.tids.remove(&tid);
                }
                Ok((tid, _)) => {
                    let _ = ptrace(libc::PTRACE_CONT, tid, 0, libc::SIGKILL as usize);
                }
                Err(_) => break,
            }
        }
    }
}

fn validate(plan: &Plan) -> Result<(CString, Vec<CString>, Vec<CString>, CString), Refusal> {
    let invalid = |detail: &str| refusal(RefusalKind::InvalidPlan, detail);
    if plan.argv.is_empty()
        || plan.executable.is_empty()
        || plan.gates.len() > 4
        || plan.max_events < 2
        || plan.max_blob_bytes == 0
    {
        return Err(invalid(
            "empty launch vector, invalid ceilings, or more than four gates",
        ));
    }
    for gate in &plan.gates {
        if gate.original_bytes.is_empty() || gate.original_bytes.len() > 15 {
            return Err(invalid("gate requires 1..15 original bytes"));
        }
        if let Some(window) = &gate.window {
            if window.max_steps == 0 || window.instructions.is_empty() {
                return Err(invalid("empty step window"));
            }
            for instruction in &window.instructions {
                if instruction.bytes.is_empty()
                    || instruction.bytes.len() > 15
                    || instruction.bytes.starts_with(&[0x0f, 0x05])
                    || instruction.bytes.contains(&0xcc)
                {
                    return Err(invalid(
                        "invalid instruction extent, syscall, or INT3 in step window",
                    ));
                }
            }
        }
    }
    for entry in &plan.env {
        let name = entry.split(|byte| *byte == b'=').next().unwrap();
        if !entry.contains(&b'=')
            || [
                b"LD_PRELOAD".as_slice(),
                b"LD_AUDIT",
                b"LD_BIND_NOW",
                b"REFERENCE_TRACE",
                b"REFERENCE_REPLAY_COMPARE",
                b"RUNNING_UNDER_RR",
            ]
            .contains(&name)
        {
            return Err(invalid(
                "missing env separator or prohibited observer/loader variable",
            ));
        }
    }
    let c = |data: &[u8]| CString::new(data).map_err(|_| invalid("interior NUL in execve vector"));
    let executable = c(&plan.executable)?;
    let elf = fs::read(Path::new(std::ffi::OsStr::from_bytes(&plan.executable)))
        .map_err(|e| invalid(&e.to_string()))?;
    if hash(&elf) != plan.executable_sha256 {
        return Err(refusal(
            RefusalKind::IdentityChanged,
            "executable hash mismatch before fork",
        ));
    }
    if elf.len() < 64 || !elf.starts_with(b"\x7fELF\x02\x01") || elf[16..20] != [2, 0, 62, 0] {
        return Err(refusal(
            RefusalKind::UnsupportedPlatform,
            "only static ELF64 x86-64 ET_EXEC address manifests are qualified",
        ));
    }
    let offset = u64::from_le_bytes(elf[32..40].try_into().unwrap());
    let size = u16::from_le_bytes(elf[54..56].try_into().unwrap()) as u64;
    let count = u16::from_le_bytes(elf[56..58].try_into().unwrap()) as u64;
    for index in 0..count {
        let start = offset
            .checked_add(index * size)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| invalid("ELF program header overflow"))?;
        let header = elf
            .get(start..start.saturating_add(4))
            .ok_or_else(|| invalid("short ELF program header"))?;
        if header == [3, 0, 0, 0] {
            return Err(refusal(
                RefusalKind::UnsupportedPlatform,
                "dynamic interpreter needs admitted module/relocation/dispatch coverage",
            ));
        }
    }
    Ok((
        executable,
        plan.argv.iter().map(|v| c(v)).collect::<Result<_, _>>()?,
        plan.env.iter().map(|v| c(v)).collect::<Result<_, _>>()?,
        c(&plan.cwd)?,
    ))
}

pub(crate) fn observe(plan: &Plan, root: &Path) -> Result<Completion, Refusal> {
    let mut receipts = Receipts::create(plan, root)?;
    let result = run(plan, &mut receipts);
    match &result {
        Ok(_) => receipts.finish(true)?,
        Err(error) => {
            receipts.event(
                0,
                0,
                0,
                None,
                None,
                "refusal",
                "rejected",
                json!({"first_failure":error}),
            )?;
            receipts.finish(false)?;
        }
    }
    result
}

fn run(plan: &Plan, receipts: &mut Receipts<'_>) -> Result<Completion, Refusal> {
    let (executable, argv, env, cwd) = validate(plan)?;
    let task_count = fs::read_dir("/proc/self/task")
        .map_err(|_| system("observer task inventory"))?
        .count();
    let own_tid = unsafe { libc::syscall(libc::SYS_gettid) };
    let existing_children = fs::read_to_string(format!("/proc/self/task/{own_tid}/children"))
        .map_err(|_| system("observer child inventory"))?;
    if task_count != 1 || !existing_children.trim().is_empty() {
        return Err(refusal(
            RefusalKind::InvalidPlan,
            "run the observer in a dedicated single-threaded process with no existing children",
        ));
    }
    let mut argv_ptrs: Vec<_> = argv.iter().map(|s| s.as_ptr()).collect();
    argv_ptrs.push(std::ptr::null());
    let mut env_ptrs: Vec<_> = env.iter().map(|s| s.as_ptr()).collect();
    env_ptrs.push(std::ptr::null());
    receipts.event(0,0,0,None,None,"launch","prepared",json!({"executable":bytes(&plan.executable),"argv":plan.argv.iter().map(|v| bytes(v)).collect::<Vec<_>>(),"env":plan.env.iter().map(|v| bytes(v)).collect::<Vec<_>>(),"cwd":bytes(&plan.cwd),"kernel":fs::read_to_string("/proc/version").map_err(|_| system("kernel identity"))?,"policy":"ptrace-syscall witness at both levels; no observer text/data/FP writes"}))?;
    let layout: Vec<_> = (0..64)
        .map(|subleaf| {
            let leaf = std::arch::x86_64::__cpuid_count(13, subleaf);
            json!({"subleaf":subleaf,"eax":leaf.eax,"ebx":leaf.ebx,"ecx":leaf.ecx,"edx":leaf.edx})
        })
        .collect();
    let cpuinfo = fs::read("/proc/cpuinfo").map_err(|_| system("CPU identity"))?;
    let cpuinfo = receipts.blob(&cpuinfo)?;
    receipts.event(0,0,0,None,None,"numeric_profile","observed",json!({"cpuid_leaf_13":layout,"cpuinfo":cpuinfo,"address_policy":"ELF ET_EXEC virtual addresses, load bias zero; dynamic code is not admitted"}))?;
    // Allocate everything before fork. The child executes only libc syscall wrappers.
    let child = unsafe { libc::syscall(libc::SYS_fork) as pid_t };
    if child < 0 {
        return Err(system("fork"));
    }
    if child == 0 {
        unsafe {
            if libc::ptrace(
                libc::PTRACE_TRACEME,
                0,
                std::ptr::null_mut::<c_void>(),
                std::ptr::null_mut::<c_void>(),
            ) == -1
            {
                libc::_exit(125);
            }
            libc::syscall(
                libc::SYS_kill,
                libc::syscall(libc::SYS_getpid),
                libc::SIGSTOP,
            );
            if libc::chdir(cwd.as_ptr()) != 0 {
                libc::_exit(126);
            }
            libc::execve(executable.as_ptr(), argv_ptrs.as_ptr(), env_ptrs.as_ptr());
            libc::_exit(127);
        }
    }
    let mut tracees = Tracees {
        tids: BTreeMap::from([(
            child,
            Thread {
                process: 1,
                logical: 1,
                birth_return_pending: true,
                ..Thread::default()
            },
        )]),
    };
    let (_, status) = wait(child)?;
    if !libc::WIFSTOPPED(status) || libc::WSTOPSIG(status) != libc::SIGSTOP {
        return Err(refusal(RefusalKind::UnexpectedStop, "missing initial stop"));
    }
    ptrace(libc::PTRACE_SETOPTIONS, child, 0, OPTIONS as usize)?;
    // Run to exec, without recording pre-exec launcher syscalls as producer operations.
    ptrace(libc::PTRACE_CONT, child, 0, 0)?;
    let (tid, status) = wait(child)?;
    if tid != child || status >> 16 != libc::PTRACE_EVENT_EXEC {
        return Err(refusal(RefusalKind::UnexpectedStop, "exec entry missing"));
    }
    let gates = if plan.level == Level::Full {
        plan.gates.as_slice()
    } else {
        &[]
    };
    let mut totals = Completion {
        processes: 1,
        threads: 1,
        events: 0,
        breakpoints: 0,
        steps: 0,
    };
    exec_identity(child, plan)?;
    let mut descriptors = BTreeMap::from([(1, Descriptors::initial(child, receipts)?)]);
    birth(
        child,
        tracees.tids.get_mut(&child).unwrap(),
        false,
        gates,
        receipts,
        "exec_entry",
    )?;
    resume(child, tracees.tids.get(&child).unwrap())?;
    while !tracees.tids.is_empty() {
        let (tid, status) = wait(-1)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = tracees.tids.entry(tid) {
            entry.insert(Thread::default());
            return Err(refusal(
                RefusalKind::UntrackedChild,
                format!("wait returned unknown tid {tid}"),
            ));
        }
        if libc::WIFEXITED(status) || libc::WIFSIGNALED(status) {
            let thread = tracees.tids.remove(&tid).unwrap();
            if !thread.exit_seen {
                return Err(refusal(
                    RefusalKind::LostEvent,
                    "exit without exit-state event",
                ));
            }
            receipts.event(
                thread.process,
                thread.logical,
                thread.seq + 1,
                None,
                None,
                "process_exit",
                "observed",
                json!({"raw_tid":tid,"wait_status":status}),
            )?;
            if !libc::WIFEXITED(status) || libc::WEXITSTATUS(status) != 0 {
                return Err(refusal(
                    RefusalKind::NonzeroExit,
                    format!("tid={tid} status={status}"),
                ));
            }
            continue;
        }
        if !libc::WIFSTOPPED(status) {
            return Err(refusal(RefusalKind::UnexpectedStop, "non-stop wait event"));
        }
        let process = tracees.tids[&tid].process;
        let shared = tracees.tids.len() > 1;
        let thread = tracees.tids.get_mut(&tid).unwrap();
        thread.deliver_signal = 0;
        let event = status >> 16;
        let signal = libc::WSTOPSIG(status);
        if thread.newborn {
            if signal != libc::SIGSTOP {
                return Err(refusal(
                    RefusalKind::UnexpectedStop,
                    "missing child initial SIGSTOP",
                ));
            }
            thread.newborn = false;
            thread.birth_return_pending = true;
            birth(tid, thread, true, gates, receipts, "thread_birth")?;
        } else if event == libc::PTRACE_EVENT_CLONE
            || event == libc::PTRACE_EVENT_FORK
            || event == libc::PTRACE_EVENT_VFORK
        {
            if thread.window.is_some() {
                return Err(refusal(
                    RefusalKind::UnsupportedInstruction,
                    "child event inside step window",
                ));
            }
            if event == libc::PTRACE_EVENT_VFORK {
                return Err(refusal(
                    RefusalKind::SharedMemoryOwnership,
                    "vfork shared address-space ownership is not qualified",
                ));
            }
            let mut child_tid = 0u64;
            ptrace(
                libc::PTRACE_GETEVENTMSG,
                tid,
                0,
                (&mut child_tid as *mut u64) as usize,
            )?;
            let child_tid = i32::try_from(child_tid)
                .map_err(|_| refusal(RefusalKind::UntrackedChild, "child tid overflow"))?;
            if child_tid <= 0 {
                return Err(refusal(RefusalKind::UntrackedChild, "invalid child tid"));
            }
            let state = snapshot(tid)?;
            thread.seq += 1;
            receipts.event(
                thread.process,
                thread.logical,
                thread.seq,
                None,
                Some(state.regs.rip),
                "child_created",
                "observed",
                json!({"parent_raw_tid":tid,"child_raw_tid":child_tid,"ptrace_event":event}),
            )?;
            unchanged(tid, &state)?;
            let parent_process = thread.process;
            totals.threads += 1;
            let process = if event == libc::PTRACE_EVENT_FORK {
                totals.processes += 1;
                totals.processes
            } else {
                parent_process
            };
            if process != parent_process {
                descriptors.insert(process, descriptors[&parent_process].clone());
            }
            if tracees
                .tids
                .insert(
                    child_tid,
                    Thread {
                        process,
                        logical: totals.threads,
                        newborn: true,
                        creation_event: receipts.seq,
                        ..Thread::default()
                    },
                )
                .is_some()
            {
                return Err(refusal(
                    RefusalKind::UntrackedChild,
                    "duplicate live child tid",
                ));
            }
        } else if event == libc::PTRACE_EVENT_EXEC {
            return Err(refusal(
                RefusalKind::IdentityChanged,
                "subsequent exec requires a separately admitted module/argv manifest",
            ));
        } else if event == libc::PTRACE_EVENT_EXIT {
            if thread.exit_seen
                || !thread.pending.as_ref().is_some_and(|call| {
                    matches!(call.number as i64, libc::SYS_exit | libc::SYS_exit_group)
                })
            {
                return Err(refusal(
                    RefusalKind::LostEvent,
                    "exit event without observed exit syscall entry",
                ));
            }
            thread.exit_seen = true;
            let state = snapshot(tid)?;
            thread.seq += 1;
            let xsave = receipts.blob(&state.xsave)?;
            receipts.event(
                thread.process,
                thread.logical,
                thread.seq,
                None,
                Some(state.regs.rip),
                "exit_state",
                "observed",
                json!({"raw_tid":tid,"xsave":xsave,"general":bytes(&state.general)}),
            )?;
            unchanged(tid, &state)?;
            thread.pending = None;
        } else if event != 0 {
            return Err(refusal(
                RefusalKind::UnexpectedStop,
                format!("unsupported ptrace event {event}"),
            ));
        } else if signal == (libc::SIGTRAP | 0x80) {
            syscall_stop(
                tid,
                thread,
                shared,
                descriptors.get_mut(&process).unwrap(),
                receipts,
            )?;
        } else if signal == libc::SIGTRAP {
            trap(tid, thread, gates, shared, receipts, &mut totals)?;
        } else if signal == libc::SIGCHLD && thread.window.is_none() {
            let state = snapshot(tid)?;
            let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
            ptrace(libc::PTRACE_GETSIGINFO, tid, 0, info.as_mut_ptr() as usize)?;
            let raw = unsafe {
                std::slice::from_raw_parts(info.as_ptr().cast::<u8>(), size_of::<libc::siginfo_t>())
            };
            thread.seq += 1;
            receipts.event(
                thread.process,
                thread.logical,
                thread.seq,
                None,
                Some(state.regs.rip),
                "signal_delivery",
                "forwarded",
                json!({"signal":signal,"siginfo":bytes(raw)}),
            )?;
            thread.deliver_signal = signal;
            unchanged(tid, &state)?;
        } else {
            return Err(refusal(
                RefusalKind::UnexpectedStop,
                format!("signal {signal} to tid {tid}; no silent signal suppression"),
            ));
        }
        resume(tid, &tracees.tids[&tid])?;
    }
    let final_executable = fs::read(Path::new(std::ffi::OsStr::from_bytes(&plan.executable)))
        .map_err(|_| system("post-exit executable identity"))?;
    if hash(&final_executable) != plan.executable_sha256 {
        return Err(refusal(
            RefusalKind::IdentityChanged,
            "executable changed during observation",
        ));
    }
    totals.events = receipts.seq + 1;
    receipts.event(
        0,
        0,
        0,
        None,
        None,
        "completion",
        "accepted",
        json!({"counts":totals,"lost_events":0,"unsupported":0}),
    )?;
    Ok(totals)
}

fn exec_identity(tid: pid_t, plan: &Plan) -> Result<(), Refusal> {
    let data = fs::read(format!("/proc/{tid}/exe")).map_err(|_| system("exec identity"))?;
    if hash(&data) != plan.executable_sha256 {
        return Err(refusal(
            RefusalKind::IdentityChanged,
            "actual exec bytes differ",
        ));
    }
    Ok(())
}
fn birth(
    tid: pid_t,
    thread: &mut Thread,
    inherited: bool,
    gates: &[Gate],
    receipts: &mut Receipts<'_>,
    kind: &str,
) -> Result<(), Refusal> {
    let state = snapshot(tid)?;
    arm(tid, gates, inherited)?;
    for gate in gates {
        if memory(tid, gate.pc, gate.original_bytes.len())? != gate.original_bytes {
            return Err(refusal(
                RefusalKind::IdentityChanged,
                format!("gate {} bytes differ", gate.site_id),
            ));
        }
    }
    thread.seq += 1;
    let xsave = receipts.blob(&state.xsave)?;
    let maps = fs::read(format!("/proc/{tid}/maps")).map_err(|_| system("maps"))?;
    let maps = receipts.blob(&maps)?;
    let parents: Vec<_> = (thread.creation_event != 0)
        .then_some(thread.creation_event)
        .into_iter()
        .collect();
    receipts.event(thread.process,thread.logical,thread.seq,None,Some(state.regs.rip),kind,"observed",json!({"parent_event_ids":parents,"raw_tid":tid,"general":bytes(&state.general),"xsave":xsave,"maps":maps,"x87_control":u16::from_le_bytes(state.xsave[0..2].try_into().unwrap()),"x87_status":u16::from_le_bytes(state.xsave[2..4].try_into().unwrap()),"x87_tag":state.xsave[4],"mxcsr":u32::from_le_bytes(state.xsave[24..28].try_into().unwrap()),"xstate_bv":u64::from_le_bytes(state.xsave[512..520].try_into().unwrap())}))?;
    unchanged(tid, &state)
}
fn resume(tid: pid_t, thread: &Thread) -> Result<(), Refusal> {
    let request = if thread.window.is_some() {
        libc::PTRACE_SINGLESTEP
    } else {
        libc::PTRACE_SYSCALL
    };
    ptrace(request, tid, 0, thread.deliver_signal as usize).map(|_| ())
}
fn trap(
    tid: pid_t,
    thread: &mut Thread,
    gates: &[Gate],
    shared: bool,
    receipts: &mut Receipts<'_>,
    totals: &mut Completion,
) -> Result<(), Refusal> {
    let state = snapshot(tid)?;
    let dr6 = debug(tid, 6)?;
    if let Some(active) = &mut thread.window {
        if dr6 & (1 << 14) == 0 {
            return Err(refusal(
                RefusalKind::UnexpectedStop,
                "not a native single-step trap",
            ));
        }
        let window = gates[active.gate].window.as_ref().unwrap();
        active.steps += 1;
        totals.steps += 1;
        let after = match active.instruction.address {
            Some(address) => memory(tid, address, 4)?,
            None => Vec::new(),
        };
        let executed = json!({"pc":active.instruction.pc,"bytes":bytes(&active.instruction.code),"memory_address":active.instruction.address,"memory_before":bytes(&active.instruction.before),"memory_after":bytes(&after)});
        if window.preserve_fp && state.xsave != active.fp_before {
            let expected = receipts.blob(&active.fp_before)?;
            let actual = receipts.blob(&state.xsave)?;
            thread.seq += 1;
            receipts.event(thread.process,thread.logical,thread.seq,Some(&gates[active.gate].site_id),Some(state.regs.rip),"numeric_divergence","rejected",json!({"parent_event_ids":[active.parent_event],"expected_xsave":expected,"actual_xsave":actual,"executed_instruction":executed}))?;
            return Err(refusal(
                RefusalKind::FloatingPointChanged,
                format!("step at {:#x}", state.regs.rip),
            ));
        }
        thread.seq += 1;
        let xsave = receipts.blob(&state.xsave)?;
        receipts.event(thread.process,thread.logical,thread.seq,Some(&gates[active.gate].site_id),Some(state.regs.rip),"native_step","observed",json!({"parent_event_ids":[active.parent_event],"step":active.steps,"general":bytes(&state.general),"xsave":xsave,"executed_instruction":executed}))?;
        active.parent_event = receipts.seq;
        if state.regs.rip == window.end_pc {
            thread.window = None;
            arm(tid, gates, true)?;
        } else {
            if active.steps >= window.max_steps {
                return Err(refusal(
                    RefusalKind::ResourceCeiling,
                    "single-step window bound",
                ));
            }
            active.instruction = check_instruction(tid, state.regs.rip, window, shared)?;
        }
    } else {
        let hits = dr6 & 15;
        if hits.count_ones() != 1 {
            return Err(refusal(
                RefusalKind::UnexpectedStop,
                format!("unclassified hardware trap DR6={dr6:#x}"),
            ));
        }
        let index = hits.trailing_zeros() as usize;
        let gate = gates
            .get(index)
            .ok_or_else(|| refusal(RefusalKind::LostEvent, "unarmed gate hit"))?;
        if state.regs.rip != gate.pc
            || memory(tid, gate.pc, gate.original_bytes.len())? != gate.original_bytes
        {
            return Err(refusal(
                RefusalKind::IdentityChanged,
                "hardware hit PC/bytes differ",
            ));
        }
        totals.breakpoints += 1;
        thread.seq += 1;
        let xsave = receipts.blob(&state.xsave)?;
        receipts.event(
            thread.process,
            thread.logical,
            thread.seq,
            Some(&gate.site_id),
            Some(gate.pc),
            "hardware_breakpoint",
            "observed",
            json!({"dr6":dr6,"general":bytes(&state.general),"xsave":xsave}),
        )?;
        if let Some(window) = &gate.window {
            let instruction = check_instruction(tid, gate.pc, window, shared)?;
            set_debug(tid, 7, 0)?;
            thread.window = Some(ActiveWindow {
                gate: index,
                steps: 0,
                fp_before: state.xsave.clone(),
                parent_event: receipts.seq,
                instruction,
            });
        }
        // Without a window Linux's RF bit suppresses precisely the faulting execution.
    }
    set_debug(tid, 6, 0)?;
    unchanged(tid, &state)
}
fn check_instruction(
    tid: pid_t,
    pc: u64,
    window: &crate::Window,
    shared: bool,
) -> Result<NativeCapture, Refusal> {
    let instruction = window
        .instructions
        .iter()
        .find(|instruction| instruction.pc == pc)
        .ok_or_else(|| {
            refusal(
                RefusalKind::UnsupportedInstruction,
                format!("unclassified PC {pc:#x}"),
            )
        })?;
    let code = instruction.bytes.as_slice();
    let supported = code == [0x90]
        || code == [0x48, 0x83, 0xc0, 0x01]
        || (code.len() == 7
            && (code.starts_with(&[0x0f, 0xae, 0x1d]) || code.starts_with(&[0x0f, 0xae, 0x15])))
        || (code.len() == 10 && code.starts_with(&[0x81, 0x0d]));
    if !supported {
        return Err(refusal(
            RefusalKind::UnsupportedInstruction,
            format!("unqualified native instruction at {pc:#x}"),
        ));
    }
    if memory(tid, pc, instruction.bytes.len())? != instruction.bytes {
        return Err(refusal(
            RefusalKind::UnsupportedInstruction,
            format!("instruction differs at {pc:#x}"),
        ));
    }
    let displacement = if code.len() == 7 {
        Some(i32::from_le_bytes(code[3..7].try_into().unwrap()))
    } else if code.len() == 10 {
        Some(i32::from_le_bytes(code[2..6].try_into().unwrap()))
    } else {
        None
    };
    if shared && displacement.is_some() {
        return Err(refusal(
            RefusalKind::SharedMemoryOwnership,
            "native memory instruction with another live task",
        ));
    }
    let address = displacement.map(|offset| {
        pc.wrapping_add(code.len() as u64)
            .wrapping_add_signed(offset as i64)
    });
    let before = match address {
        Some(address) => memory(tid, address, 4)?,
        None => Vec::new(),
    };
    Ok(NativeCapture {
        pc,
        code: code.to_vec(),
        address,
        before,
    })
}

fn syscall_stop(
    tid: pid_t,
    thread: &mut Thread,
    shared: bool,
    descriptors: &mut Descriptors,
    receipts: &mut Receipts<'_>,
) -> Result<(), Refusal> {
    // GET_SYSCALL_INFO distinguishes entry/exit without a fragile per-thread toggle.
    let mut info = [0u64; 16];
    let got = ptrace(
        libc::PTRACE_GET_SYSCALL_INFO,
        tid,
        size_of::<[u64; 16]>(),
        info.as_mut_ptr() as usize,
    )?;
    if got < 24 {
        return Err(refusal(RefusalKind::LostEvent, "short syscall info"));
    }
    let operation = info[0] & 0xff;
    let architecture = info[0] >> 32;
    if architecture != 0xc000003e {
        return Err(refusal(
            RefusalKind::UnsupportedSyscall,
            format!("audit architecture {architecture:#x}"),
        ));
    }
    let state = snapshot(tid)?;
    thread.seq += 1;
    if operation == 1 {
        if thread.pending.is_some() {
            return Err(refusal(
                RefusalKind::LostEvent,
                "syscall entry without preceding exit",
            ));
        }
        thread.birth_return_pending = false;
        if got < 80 {
            return Err(refusal(RefusalKind::LostEvent, "short syscall entry"));
        }
        let number = info[3];
        if matches!(number as i64, libc::SYS_exit | libc::SYS_exit_group)
            && !descriptors.live.is_empty()
        {
            return Err(refusal(RefusalKind::UnsupportedSyscall,format!("exit at {:#x} with live descriptors {:?}; kernel-internal release order is not witnessed",state.regs.rip,descriptors.live)));
        }
        let args: [u64; 6] = info[4..10].try_into().unwrap();
        let mut payload = syscall_entry(tid, number, args, shared, receipts)?;
        payload["fd_table_before"] = json!(descriptors.live);
        receipts.event(
            thread.process,
            thread.logical,
            thread.seq,
            None,
            Some(state.regs.rip),
            "syscall_entry",
            "observed",
            json!({"number":number,"arguments_u64":args,"capture":payload}),
        )?;
        thread.pending = Some(Syscall {
            number,
            args,
            payload,
            entry_event: receipts.seq,
        });
    } else if operation == 2 {
        if got < 33 {
            return Err(refusal(RefusalKind::LostEvent, "short syscall exit"));
        }
        let result = info[3] as i64;
        let Some(call) = thread.pending.take() else {
            // A new fork/clone task first stops at the parent's syscall return.
            if result != 0 || !thread.birth_return_pending {
                return Err(refusal(RefusalKind::LostEvent, "uncorrelated syscall exit"));
            }
            thread.birth_return_pending = false;
            receipts.event(
                thread.process,
                thread.logical,
                thread.seq,
                None,
                Some(state.regs.rip),
                "lifecycle_syscall_return",
                "observed",
                json!({"result_i64":result}),
            )?;
            return unchanged(tid, &state);
        };
        let mut data = syscall_exit(tid, &call, result, shared, receipts)?;
        if result >= 0 {
            match call.number as i64 {
                libc::SYS_open
                | libc::SYS_openat
                | libc::SYS_dup
                | libc::SYS_dup2
                | libc::SYS_dup3 => {
                    data["new_fd_generation"] = json!(descriptors.open(result as u64));
                }
                libc::SYS_close => {
                    data["closed_fd_generation"] = json!(descriptors.live.remove(&call.args[0]));
                }
                _ => {}
            }
        }
        data["fd_table_after"] = json!(descriptors.live);
        receipts.event(thread.process,thread.logical,thread.seq,None,Some(state.regs.rip),"syscall_exit","observed",json!({"parent_event_ids":[call.entry_event],"number":call.number,"arguments_u64":call.args,"result_i64":result,"is_error":info[4]&0xff,"entry_capture":call.payload,"capture":data}))?;
    } else {
        return Err(refusal(
            RefusalKind::LostEvent,
            format!("unsupported syscall-info operation {operation}"),
        ));
    }
    unchanged(tid, &state)
}

fn syscall_entry(
    tid: pid_t,
    number: u64,
    args: [u64; 6],
    shared: bool,
    receipts: &mut Receipts<'_>,
) -> Result<Value, Refusal> {
    let nr = number as i64;
    if shared
        && matches!(
            nr,
            libc::SYS_read
                | libc::SYS_write
                | libc::SYS_pread64
                | libc::SYS_pwrite64
                | libc::SYS_getdents64
                | libc::SYS_fstat
                | libc::SYS_lseek
                | libc::SYS_close
                | libc::SYS_open
                | libc::SYS_openat
                | libc::SYS_unlink
                | libc::SYS_unlinkat
                | libc::SYS_mkdir
                | libc::SYS_mkdirat
                | libc::SYS_rmdir
                | libc::SYS_access
                | libc::SYS_faccessat
                | libc::SYS_stat
                | libc::SYS_lstat
                | libc::SYS_newfstatat
                | libc::SYS_readlink
                | libc::SYS_readlinkat
                | libc::SYS_fsync
                | libc::SYS_fdatasync
                | libc::SYS_dup
                | libc::SYS_dup2
                | libc::SYS_dup3
                | libc::SYS_ftruncate
        )
    {
        return Err(refusal(RefusalKind::SharedMemoryOwnership,"concurrent filesystem operations need a kernel linearization witness; workers are not serialized"));
    }
    let mut data = json!({});
    let path_arg = match nr {
        libc::SYS_open
        | libc::SYS_unlink
        | libc::SYS_access
        | libc::SYS_mkdir
        | libc::SYS_rmdir
        | libc::SYS_readlink
        | libc::SYS_stat
        | libc::SYS_lstat => Some(0),
        libc::SYS_openat
        | libc::SYS_unlinkat
        | libc::SYS_mkdirat
        | libc::SYS_newfstatat
        | libc::SYS_readlinkat
        | libc::SYS_faccessat => Some(1),
        _ => None,
    };
    if let Some(index) = path_arg {
        if shared {
            return Err(refusal(
                RefusalKind::SharedMemoryOwnership,
                "concurrent mutable pathname",
            ));
        }
        data["path"] = bytes(&c_string(tid, args[index])?);
    }
    match nr {
        libc::SYS_write | libc::SYS_pwrite64 => {
            if shared {
                return Err(refusal(
                    RefusalKind::SharedMemoryOwnership,
                    "concurrent mutable write buffer",
                ));
            }
            let length = usize::try_from(args[2])
                .map_err(|_| refusal(RefusalKind::ResourceCeiling, "write length overflow"))?;
            data["requested_bytes"] = receipts.blob(&memory(tid, args[1], length)?)?;
        }
        libc::SYS_read
        | libc::SYS_pread64
        | libc::SYS_getdents64
        | libc::SYS_fstat
        | libc::SYS_lseek
        | libc::SYS_close
        | libc::SYS_open
        | libc::SYS_openat
        | libc::SYS_unlink
        | libc::SYS_unlinkat
        | libc::SYS_mkdir
        | libc::SYS_mkdirat
        | libc::SYS_rmdir
        | libc::SYS_access
        | libc::SYS_faccessat
        | libc::SYS_stat
        | libc::SYS_lstat
        | libc::SYS_newfstatat
        | libc::SYS_readlink
        | libc::SYS_readlinkat
        | libc::SYS_fsync
        | libc::SYS_fdatasync
        | libc::SYS_dup
        | libc::SYS_dup2
        | libc::SYS_dup3
        | libc::SYS_ftruncate
        | libc::SYS_brk
        | libc::SYS_munmap
        | libc::SYS_mprotect
        | libc::SYS_madvise
        | libc::SYS_arch_prctl
        | libc::SYS_set_tid_address
        | libc::SYS_set_robust_list
        | libc::SYS_rseq
        | libc::SYS_rt_sigaction
        | libc::SYS_rt_sigprocmask
        | libc::SYS_sigaltstack
        | libc::SYS_rt_sigreturn
        | libc::SYS_getpid
        | libc::SYS_gettid
        | libc::SYS_prlimit64
        | libc::SYS_getrandom
        | libc::SYS_fork
        | libc::SYS_clone
        | libc::SYS_clone3
        | libc::SYS_wait4
        | libc::SYS_futex
        | libc::SYS_exit
        | libc::SYS_exit_group
        | libc::SYS_clock_gettime
        | libc::SYS_sched_yield
        | libc::SYS_getuid
        | libc::SYS_geteuid
        | libc::SYS_getgid
        | libc::SYS_getegid
        | libc::SYS_uname
        | libc::SYS_sched_getaffinity => {}
        libc::SYS_mmap => {
            if args[3] & libc::MAP_ANONYMOUS as u64 == 0 {
                return Err(refusal(
                    RefusalKind::UnsupportedSyscall,
                    "file-backed mmap requires mapped access witness",
                ));
            }
        }
        _ => {
            return Err(refusal(
                RefusalKind::UnsupportedSyscall,
                format!("syscall {number} at entry"),
            ))
        }
    }
    // Shared VM without CLONE_THREAD is a distinct ownership model and is refused.
    if nr == libc::SYS_clone
        && args[0] & libc::CLONE_VM as u64 != 0
        && args[0] & libc::CLONE_THREAD as u64 == 0
    {
        return Err(refusal(
            RefusalKind::SharedMemoryOwnership,
            "CLONE_VM without CLONE_THREAD",
        ));
    }
    // clone3 structure ownership and size are not inferred from clone's ABI.
    if nr == libc::SYS_clone3 {
        return Err(refusal(
            RefusalKind::UnsupportedSyscall,
            "clone3 structure ABI is not qualified",
        ));
    }
    Ok(data)
}
fn syscall_exit(
    tid: pid_t,
    call: &Syscall,
    result: i64,
    shared: bool,
    receipts: &mut Receipts<'_>,
) -> Result<Value, Refusal> {
    if result < 0 {
        return Ok(json!({}));
    }
    let args = call.args;
    let (address, length) = match call.number as i64 {
        libc::SYS_read | libc::SYS_pread64 | libc::SYS_getdents64 => {
            if result as u64 > args[2] {
                return Err(refusal(
                    RefusalKind::LostEvent,
                    "return exceeds requested transfer",
                ));
            }
            (args[1], result as usize)
        }
        libc::SYS_readlink => (args[1], result as usize),
        libc::SYS_readlinkat => (args[2], result as usize),
        libc::SYS_fstat | libc::SYS_stat | libc::SYS_lstat => (args[1], size_of::<libc::stat>()),
        libc::SYS_newfstatat => (args[2], size_of::<libc::stat>()),
        _ => return Ok(json!({})),
    };
    if shared {
        return Err(refusal(
            RefusalKind::SharedMemoryOwnership,
            "concurrent mutable syscall output",
        ));
    }
    Ok(json!({"returned_bytes":receipts.blob(&memory(tid,address,length)?)?}))
}
