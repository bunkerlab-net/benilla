# Extractor observer

Original code, licensed MIT OR Apache-2.0. This observer does not admit an
extractor run or establish historical F1 provenance.

## Supported observation

The Linux x86-64 parent prepares exact `execve` argv and environment byte
vectors, including duplicate environment keys and non-UTF-8 bytes. Its child
uses only syscall wrappers before exec. All allocation, interpretation, hashing,
and receipt encoding happen in the parent.

`kernel-witness` records ptrace syscall entry and exit without semantic probes.
`full` adds up to four DR0-3 execution gates, using DR7 and no text patches.
Windows step native instructions from an explicit PC/byte whitelist. The
qualified instruction forms are NOP, ADD RAX, STMXCSR, LDMXCSR, and OR to a
RIP-relative dword. An unknown instruction or edge refuses before that step.
This is not a general x86 decoder or an exhaustive producer control-flow cover.

Every stopped task has two independently read general-register and full XSAVE
snapshots before resume. The parent checks standard XSAVE layout and component
bounds against CPUID leaf 13. Exec, thread birth, gates, steps, and exit retain
numeric state. The observer never restores or writes target general/FP state.
A window can require its complete XSAVE bytes to remain unchanged.

Clone and fork tasks remain stopped until their hardware coverage is armed.
Workers run concurrently. SIGCHLD is recorded and forwarded unchanged; other
signal delivery and vfork refuse. Exit-state events and syscall returns are
conserved, including the special exec/child return boundary.
Run the observer in its own single-threaded process with no existing children.

The filesystem witness retains exact path bytes with NUL, arguments, failures,
requested write buffers, returned read/directory/stat bytes, descriptor
lifetimes, and syscall order. Short memory reads refuse. Transfers above 16 MiB
refuse. A filesystem operation with another live traced task refuses because
ptrace stop order alone cannot prove kernel operation order or shared-buffer
ownership. The observer does not serialize workers to bypass that restriction.
Exit with any live descriptor also refuses before the syscall because ptrace
does not capture kernel-internal implicit-close order. Accepted fixtures close
every inherited descriptor explicitly before starting workers.

Static ELF64 ET_EXEC manifests are the qualified address ABI. PIE, dynamic
interpreters, file-backed mmap, clone3, later exec, unsupported syscalls, and
unqualified ownership refuse. Dynamic module/IFUNC/libm dispatch is not admitted.
The four real extractors must not be run with this fixture-only qualification.

## Receipts

Run `benilla-extractor-observer PLAN.json NEW_RECEIPT_DIRECTORY`. `Plan` in
`src/lib.rs` defines the input schema; byte vectors are JSON integer arrays,
not Unicode strings. The directory must not exist. It is private to its owner.

`products.jsonl` uses the observer design's common envelope. ET_EXEC PCs are ELF
virtual addresses with load bias zero. Unknown source ownership stays empty,
not inferred. `observer_seq` orders acquisition, not concurrent source causes.
Each record hashes the preceding JSON bytes without its newline. Blob hashes
frame the data with its u64 little-endian length. `completion.json` binds the
complete newline-delimited stream, count, last-record hash, and acceptance.
Refusals retain the complete prefix and reject the attempt permanently. Event
and blob ceilings reject rather than truncate. Errors kill and reap tracees.

## Authored qualification

Build only this crate through the repository's `mbx` wrapper. Then run
`scripts/qualify-extractor-observer.py` with `--observer` and a fresh `--output`
root. The script verifies the locally available proven builder image's exact
digest. It never pulls an image. It compiles only `fixtures/mechanisms.c`, with
read-only sources, no network, and compiler scratch confined to the new root.
The compiled fixtures run on the host kernel so ptrace observes native state.

For each fixture, A and C use the syscall witness alone. B uses the same witness
plus hardware gates and native steps. Qualification compares product bytes,
every exercised kernel file-operation occurrence, and complete per-thread exit
XSAVE bytes. It checks breakpoint order, six native steps, fork/thread birth,
duplicate/raw environment vectors, and typed perturbation/unknown-edge refusal.
Negative cases also exercise unsupported syscalls, concurrent filesystem access,
event ceilings, mismatched original instruction bytes, and implicit closes.
The numeric matrix covers 64 x87/SSE rounding and FTZ/DAZ combinations, six
individual sticky exception flags, and all six flags together. Exception masks
remain enabled. This does not qualify every possible unmasked exception state,
signal handler, CPU, kernel, or producer input.

The fixture explicitly defines x87 and SSE in-use state before its probes.
This avoids comparing unspecified exec-time XSAVE initial-state bookkeeping.
The observer still retains raw exec snapshots and never normalizes differences.

Workspace tests exercise public refusal and immutable-attempt contracts.
`scripts/test-observer-manifest.py` checks the conservative anchor classes in
both local gates and CI. The Docker-dependent qualification is separate and
requires the approved image already present.

## Real-tool admission

`scripts/observer-manifest.py` reads ELF metadata, disassembly, and DWARF without
executing the inspected file. It refuses all nine semantic obligations until
complete control-flow, inline-copy, location, and memory-ownership evidence
exists. `--classify-existing` adds an exclusive supplemental manifest with
DWARF, symbol-only, or missing-anchor classes. Anchors never grant admission.

Before requesting real-tool qualification, complete every refused obligation,
PIE/module/dispatch support, a lossless concurrent kernel witness, and the
unqualified syscall/child/numeric cases that the actual producer can reach.
Per-thread ptrace stop order cannot replace a kernel witness at filesystem
operation linearization points. That backend must retain complete payloads and
detect loss without serializing workers. A nonempty MoveMapGen run reaching
worker file I/O while its controller lives cannot complete with the current
refusal policy, even with `--threads 1`. No-work invocations are not evidence
that this restriction permits the planned complete navigation extraction.
Then obtain owner approval for exact observer and manifest hashes, the recovered
quartet, image/kernel/CPU/privileges, reduced numeric witnesses, fresh A/B/C roots,
raw-data privacy, resource ceilings, and all three complete extraction pipelines.
Use `ad -f 1`, keep RefTrace variables absent, and mount sources/install/client
read-only. Approval for authored fixtures is not approval for that execution.
