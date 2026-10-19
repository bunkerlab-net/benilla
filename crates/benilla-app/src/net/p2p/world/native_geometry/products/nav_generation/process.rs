// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded execution of the trusted, packaged generator, never an executable sandbox.
//! Only the binary protocol crosses this boundary; paths and stderr are not diagnostics.

use super::GenerationError;
use benilla_native_nav_generator::{GenerationResponse, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES};
use std::{
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_STDERR_BYTES: usize = 64 * 1024;
const PIPE_CHUNK: usize = 8192;
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// The caller selects a trusted packaged binary by absolute path. No shell, arguments,
/// inherited environment configuration, source paths, or output paths are supplied.
pub(super) fn run(
    helper: &Path,
    request: Vec<u8>,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> Result<GenerationResponse, GenerationError> {
    check(deadline, cancelled)?;
    if request.len() > MAX_REQUEST_BYTES {
        return Err(GenerationError::Protocol);
    }
    if !helper.is_absolute() {
        return Err(GenerationError::Helper);
    }
    let mut command = Command::new(helper);
    command
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    let bytes = unix::run(command, request, deadline, cancelled)?;
    #[cfg(not(unix))]
    let bytes = portable::run(command, request, deadline, cancelled)?;
    check(deadline, cancelled)?;
    let response = GenerationResponse::decode(&bytes).map_err(|_| GenerationError::Protocol)?;
    check(deadline, cancelled)?;
    Ok(response)
}

fn check(deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), GenerationError> {
    if cancelled() {
        Err(GenerationError::Cancelled)
    } else if Instant::now() >= deadline {
        Err(GenerationError::Deadline)
    } else {
        Ok(())
    }
}

fn pause(deadline: Instant) {
    thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
}

fn append(bytes: &mut Vec<u8>, incoming: &[u8], limit: usize) -> Result<(), GenerationError> {
    let needed = bytes.len() + incoming.len();
    if needed > limit {
        return Err(GenerationError::Protocol);
    }
    if needed > bytes.capacity() {
        // Ordinary Vec growth can double an irregular capacity past the byte cap.
        let capacity = needed.next_power_of_two().min(limit);
        bytes
            .try_reserve_exact(capacity - bytes.len())
            .map_err(|_| GenerationError::Io)?;
    }
    bytes.extend_from_slice(incoming);
    Ok(())
}

/// Every error path, including failure to configure pipes, kills and reaps the child.
/// OS termination/reaping is required cleanup, not another cooperative helper phase.
struct Running {
    child: Child,
    reaped: bool,
    #[cfg(unix)]
    process_group: bool,
}

impl Drop for Running {
    fn drop(&mut self) {
        if !self.reaped {
            #[cfg(unix)]
            if self.process_group {
                // The unreaped leader keeps this OS-assigned group ID from being reused.
                // A conforming packaged helper does not fork; this also closes inherited
                // pipes in same-group descendants of an accidentally broken helper.
                unsafe {
                    libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
                }
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::{fd::AsRawFd, unix::process::CommandExt};

    fn nonblocking(pipe: &impl AsRawFd) -> Result<(), GenerationError> {
        let fd = pipe.as_raw_fd();
        // These are our owned pipe ends, never the child's end or a shared file offset.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
        {
            return Err(GenerationError::Io);
        }
        Ok(())
    }

    /// A finite burst keeps continuous output from starving cancellation and stdin.
    fn drain<R: Read>(
        pipe: &mut Option<R>,
        total: &mut usize,
        mut retain: Option<&mut Vec<u8>>,
        limit: usize,
    ) -> Result<bool, GenerationError> {
        let mut progress = false;
        let mut scratch = [0; PIPE_CHUNK];
        for _ in 0..8 {
            let Some(reader) = pipe.as_mut() else {
                break;
            };
            match reader.read(&mut scratch) {
                Ok(0) => {
                    pipe.take();
                    return Ok(true);
                }
                Ok(len) => {
                    if len > limit - *total {
                        return Err(if retain.is_some() {
                            GenerationError::Protocol
                        } else {
                            GenerationError::Helper
                        });
                    }
                    *total += len;
                    if let Some(bytes) = retain.as_deref_mut() {
                        append(bytes, &scratch[..len], limit)?;
                    }
                    progress = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => break,
                Err(_) => return Err(GenerationError::Io),
            }
        }
        Ok(progress)
    }

    pub(super) fn run(
        mut command: Command,
        request: Vec<u8>,
        deadline: Instant,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<u8>, GenerationError> {
        command.process_group(0);
        let mut running = Running {
            child: command.spawn().map_err(|_| GenerationError::Io)?,
            reaped: false,
            process_group: true,
        };
        let mut stdin = Some(running.child.stdin.take().ok_or(GenerationError::Io)?);
        let mut stdout = Some(running.child.stdout.take().ok_or(GenerationError::Io)?);
        let mut stderr = Some(running.child.stderr.take().ok_or(GenerationError::Io)?);
        nonblocking(stdin.as_ref().unwrap())?;
        nonblocking(stdout.as_ref().unwrap())?;
        nonblocking(stderr.as_ref().unwrap())?;
        let (mut written, mut stdout_len, mut stderr_len) = (0, 0, 0);
        let mut bytes = Vec::new();
        loop {
            check(deadline, cancelled)?;
            let mut progress = false;
            if written == request.len() {
                progress |= stdin.take().is_some();
            } else if let Some(writer) = stdin.as_mut() {
                let end = request.len().min(written + 8 * PIPE_CHUNK);
                match writer.write(&request[written..end]) {
                    Ok(0) => return Err(GenerationError::Helper),
                    Ok(len) => {
                        written += len;
                        progress = true;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                        return Err(GenerationError::Helper);
                    }
                    Err(_) => return Err(GenerationError::Io),
                }
            }
            progress |= drain(
                &mut stdout,
                &mut stdout_len,
                Some(&mut bytes),
                MAX_RESPONSE_BYTES,
            )?;
            progress |= drain(&mut stderr, &mut stderr_len, None, MAX_STDERR_BYTES)?;
            // Do not reap an exited group leader while descendants might hold pipes:
            // the deadline must still be able to kill that exact reserved group ID.
            if stdin.is_none() && stdout.is_none() && stderr.is_none() {
                if let Some(status) = running.child.try_wait().map_err(|_| GenerationError::Io)? {
                    running.reaped = true;
                    return if status.success() {
                        Ok(bytes)
                    } else {
                        Err(GenerationError::Helper)
                    };
                }
            }
            if !progress {
                pause(deadline);
            }
        }
    }
}

/// Non-Unix fallback: three bounded pipe workers, no blocking join. The packaged
/// helper never spawns children. If a broken helper leaves inherited pipe handles,
/// this still returns on deadline after killing/reaping the direct child, but cannot
/// terminate those descendants portably. A lease prevents repeated calls from
/// accumulating stranded workers. Healthy contention waits within the caller's
/// deadline; after its owner returns, further calls fail until all workers exit.
#[cfg(any(not(unix), test))]
mod portable {
    use super::*;
    use std::sync::{
        mpsc::{self, TryRecvError},
        Mutex,
    };

    struct State {
        active: bool,
        workers: usize,
    }
    static STATE: Mutex<State> = Mutex::new(State {
        active: false,
        workers: 0,
    });

    struct Lease;
    impl Lease {
        fn acquire(
            deadline: Instant,
            cancelled: &impl Fn() -> bool,
        ) -> Result<Self, GenerationError> {
            loop {
                check(deadline, cancelled)?;
                {
                    let mut state = STATE.lock().unwrap();
                    if !state.active {
                        if state.workers != 0 {
                            return Err(GenerationError::Helper);
                        }
                        state.active = true;
                        return Ok(Self);
                    }
                }
                pause(deadline);
            }
        }
    }
    impl Drop for Lease {
        fn drop(&mut self) {
            STATE.lock().unwrap().active = false;
        }
    }

    struct WorkerLease;
    impl WorkerLease {
        fn new() -> Self {
            STATE.lock().unwrap().workers += 1;
            Self
        }
    }
    impl Drop for WorkerLease {
        fn drop(&mut self) {
            STATE.lock().unwrap().workers -= 1;
        }
    }

    enum Event {
        Written(Result<(), GenerationError>),
        Stdout(Result<Vec<u8>, GenerationError>),
        Stderr(Result<(), GenerationError>),
    }

    pub(super) fn read(
        mut pipe: impl Read,
        retain: bool,
        limit: usize,
    ) -> Result<Vec<u8>, GenerationError> {
        let mut total = 0;
        let mut bytes = Vec::new();
        let mut scratch = [0; PIPE_CHUNK];
        loop {
            match pipe.read(&mut scratch) {
                Ok(0) => return Ok(bytes),
                Ok(len) => {
                    if len > limit - total {
                        return Err(if retain {
                            GenerationError::Protocol
                        } else {
                            GenerationError::Helper
                        });
                    }
                    total += len;
                    if retain {
                        append(&mut bytes, &scratch[..len], limit)?;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return Err(GenerationError::Io),
            }
        }
    }

    pub(super) fn run(
        mut command: Command,
        request: Vec<u8>,
        deadline: Instant,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<u8>, GenerationError> {
        let _lease = Lease::acquire(deadline, cancelled)?;
        let mut running = Running {
            child: command.spawn().map_err(|_| GenerationError::Io)?,
            reaped: false,
            #[cfg(unix)]
            process_group: false,
        };
        let mut stdin = running.child.stdin.take().ok_or(GenerationError::Io)?;
        let stdout = running.child.stdout.take().ok_or(GenerationError::Io)?;
        let stderr = running.child.stderr.take().ok_or(GenerationError::Io)?;
        let (sender, receiver) = mpsc::sync_channel(3);
        let (writer_sender, writer_lease) = (sender.clone(), WorkerLease::new());
        thread::Builder::new()
            .name("native-nav-stdin".into())
            .spawn(move || {
                let result = stdin.write_all(&request).map_err(|error| {
                    if error.kind() == io::ErrorKind::BrokenPipe {
                        GenerationError::Helper
                    } else {
                        GenerationError::Io
                    }
                });
                drop(stdin);
                drop(writer_lease);
                let _ = writer_sender.send(Event::Written(result));
            })
            .map_err(|_| GenerationError::Io)?;
        let (stdout_sender, stdout_lease) = (sender.clone(), WorkerLease::new());
        thread::Builder::new()
            .name("native-nav-stdout".into())
            .spawn(move || {
                let result = read(stdout, true, MAX_RESPONSE_BYTES);
                drop(stdout_lease);
                let _ = stdout_sender.send(Event::Stdout(result));
            })
            .map_err(|_| GenerationError::Io)?;
        let stderr_lease = WorkerLease::new();
        thread::Builder::new()
            .name("native-nav-stderr".into())
            .spawn(move || {
                let result = read(stderr, false, MAX_STDERR_BYTES).map(|_| ());
                drop(stderr_lease);
                let _ = sender.send(Event::Stderr(result));
            })
            .map_err(|_| GenerationError::Io)?;
        let (mut written, mut stderr_done, mut stdout_done) = (false, false, false);
        let mut bytes = Vec::new();
        loop {
            check(deadline, cancelled)?;
            match receiver.try_recv() {
                Ok(Event::Written(result)) => {
                    result?;
                    written = true;
                }
                Ok(Event::Stdout(result)) => {
                    bytes = result?;
                    stdout_done = true;
                }
                Ok(Event::Stderr(result)) => {
                    result?;
                    stderr_done = true;
                }
                Err(TryRecvError::Disconnected) if !(written && stderr_done && stdout_done) => {
                    return Err(GenerationError::Io);
                }
                Err(_) => {}
            }
            if written && stderr_done && stdout_done {
                if let Some(status) = running.child.try_wait().map_err(|_| GenerationError::Io)? {
                    running.reaped = true;
                    return if status.success() {
                        Ok(bytes)
                    } else {
                        Err(GenerationError::Helper)
                    };
                }
            }
            pause(deadline);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_rejects_cancelled_expired_oversized_and_path_search() {
        let never_run = Path::new("/missing-native-nav-helper");
        let future = Instant::now() + Duration::from_secs(10);
        assert!(matches!(
            run(never_run, vec![], future, &|| true),
            Err(GenerationError::Cancelled)
        ));
        assert!(matches!(
            run(never_run, vec![], Instant::now(), &|| false),
            Err(GenerationError::Deadline)
        ));
        assert!(matches!(
            run(never_run, vec![0; MAX_REQUEST_BYTES + 1], future, &|| false),
            Err(GenerationError::Protocol)
        ));
        assert!(matches!(
            run(Path::new("native-nav-helper"), vec![], future, &|| false),
            Err(GenerationError::Helper)
        ));
    }

    #[test]
    fn portable_reader_retains_only_stdout_and_rejects_excess() {
        assert_eq!(portable::read(&b"abcd"[..], true, 4).unwrap(), b"abcd");
        assert!(portable::read(&b"abcd"[..], false, 4).unwrap().is_empty());
        assert!(matches!(
            portable::read(&b"abcde"[..], true, 4),
            Err(GenerationError::Protocol)
        ));
        assert!(matches!(
            portable::read(&b"abcde"[..], false, 4),
            Err(GenerationError::Helper)
        ));
        let mut bytes = Vec::new();
        for chunk in [3, 4, 4, 6, 2] {
            append(&mut bytes, &vec![0; chunk], 19).unwrap();
            assert!(bytes.capacity() <= 19);
        }
        assert!(matches!(
            append(&mut bytes, &[0], 19),
            Err(GenerationError::Protocol)
        ));
        assert_eq!(bytes.len(), 19);
    }

    #[cfg(unix)]
    mod unix_helpers {
        use super::*;
        use std::{
            fs,
            os::unix::fs::PermissionsExt,
            path::PathBuf,
            sync::{
                atomic::{AtomicU64, Ordering},
                Mutex,
            },
            time::{SystemTime, UNIX_EPOCH},
        };

        // A concurrent fork can transiently inherit another fixture's open writer
        // before exec closes it, making that just-written script ETXTBSY on Linux.
        static HELPER_TEST: Mutex<()> = Mutex::new(());

        struct Helper(PathBuf);
        impl Helper {
            fn new(body: &str) -> Self {
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let directory = std::env::temp_dir().join(format!(
                    "benilla-nav-process-{}-{}-{}",
                    std::process::id(),
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&directory).unwrap();
                let path = directory.join("helper");
                fs::write(
                    &path,
                    format!("#!/usr/bin/python3\nimport os, sys, struct, time\n{body}\n"),
                )
                .unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
                Self(path)
            }
            fn run(&self, request: Vec<u8>) -> Result<GenerationResponse, GenerationError> {
                run(
                    &self.0,
                    request,
                    Instant::now() + Duration::from_secs(5),
                    &|| false,
                )
            }
            fn command(&self) -> Command {
                let mut command = Command::new(&self.0);
                command
                    .env_clear()
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                command
            }
            fn assert_reaped(&self) {
                let pid: libc::pid_t = fs::read_to_string(self.0.parent().unwrap().join("pid"))
                    .unwrap()
                    .parse()
                    .unwrap();
                let mut status = 0;
                assert_eq!(
                    unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
                    -1
                );
                assert_eq!(
                    io::Error::last_os_error().raw_os_error(),
                    Some(libc::ECHILD)
                );
            }
        }
        impl Drop for Helper {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(self.0.parent().unwrap());
            }
        }
        const EMPTY: &str = "b'BNAVOUT1' + struct.pack('<IIIIQ', 1, 28, 0, 0, 0) + bytes(28)";

        #[test]
        fn drains_both_outputs_before_helper_reads_large_stdin() {
            let _serial = HELPER_TEST.lock().unwrap();
            let helper = Helper::new("sys.stderr.buffer.write(bytes(65536))\nsys.stderr.buffer.flush()\nsys.stdout.buffer.write(b'BNAVOUT1' + struct.pack('<IIIIQ', 0, 28, 262144, 1, 0) + bytes(28 + 262144))\nsys.stdout.buffer.flush()\nassert len(sys.stdin.buffer.read()) == 1048576");
            let result = helper.run(vec![42; 1_048_576]).unwrap();
            assert_eq!(result.mmtile.len(), 262_144);
            assert_eq!(result.polygon_count, 1);
        }

        #[test]
        fn exact_framing_and_successful_exit_are_both_mandatory() {
            let _serial = HELPER_TEST.lock().unwrap();
            for (suffix, protocol) in [(" + b'x'", true), ("[:-1]", true), ("", false)] {
                let exit = if protocol { "" } else { "\nsys.exit(7)" };
                let helper = Helper::new(&format!(
                    "sys.stdin.buffer.read()\nsys.stdout.buffer.write(({EMPTY}){suffix}){exit}"
                ));
                let result = helper.run(vec![]);
                if protocol {
                    assert!(matches!(result, Err(GenerationError::Protocol)));
                } else {
                    assert!(matches!(result, Err(GenerationError::Helper)));
                }
            }
            let helper = Helper::new(&format!(
                "sys.stdin.buffer.read()\nsys.stdout.buffer.write({EMPTY})"
            ));
            assert_eq!(
                helper.run(vec![]).unwrap().status,
                benilla_native_nav_generator::GenerationStatus::Empty
            );
        }

        #[test]
        fn overflow_is_rejected_without_waiting_for_exit() {
            let _serial = HELPER_TEST.lock().unwrap();
            for (stream, count, protocol) in [
                ("stdout", MAX_RESPONSE_BYTES + 1, true),
                ("stderr", MAX_STDERR_BYTES + 1, false),
            ] {
                // No normal exit can satisfy this fixture; rejection must kill and reap it.
                let helper = Helper::new(&format!("with open(os.path.join(os.path.dirname(__file__), 'pid'), 'w') as f:\n    f.write(str(os.getpid()))\nsys.{stream}.buffer.write(bytes({count}))\nsys.{stream}.buffer.flush()\nwhile True:\n    time.sleep(10)"));
                let result = helper.run(vec![]);
                if protocol {
                    assert!(
                        matches!(result, Err(GenerationError::Protocol)),
                        "{result:?}"
                    );
                } else {
                    assert!(matches!(result, Err(GenerationError::Helper)), "{result:?}");
                }
                helper.assert_reaped();
            }
        }

        #[test]
        fn deadline_and_cancellation_interrupt_a_blocked_stdin() {
            let _serial = HELPER_TEST.lock().unwrap();
            for cancel in [false, true] {
                let helper = Helper::new("time.sleep(10)");
                let began = Instant::now();
                let result = run(
                    &helper.0,
                    vec![0; MAX_REQUEST_BYTES],
                    began + Duration::from_millis(120),
                    &|| cancel && began.elapsed() >= Duration::from_millis(40),
                );
                assert!(began.elapsed() < Duration::from_secs(1));
                if cancel {
                    assert!(matches!(result, Err(GenerationError::Cancelled)));
                } else {
                    assert!(matches!(result, Err(GenerationError::Deadline)));
                }
            }
        }

        #[test]
        fn inherited_pipes_do_not_extend_deadline_or_leave_leader_unreaped() {
            let _serial = HELPER_TEST.lock().unwrap();
            let helper = Helper::new(
                "with open(os.path.join(os.path.dirname(__file__), 'pid'), 'w') as f:\n    f.write(str(os.getpid()))\npid = os.fork()\nif pid == 0:\n    time.sleep(10)\nelse:\n    os._exit(0)",
            );
            let began = Instant::now();
            let result = run(
                &helper.0,
                vec![],
                began + Duration::from_millis(120),
                &|| false,
            );
            assert!(
                matches!(result, Err(GenerationError::Deadline)),
                "{result:?}"
            );
            assert!(began.elapsed() < Duration::from_secs(1));
            helper.assert_reaped();
        }

        #[test]
        fn portable_healthy_contention_waits_with_deadline_and_cancellation() {
            let _serial = HELPER_TEST.lock().unwrap();
            let first = Helper::new(&format!(
                "sys.stdin.buffer.read()\nwith open(os.path.join(os.path.dirname(__file__), 'pid'), 'w') as f:\n    f.write(str(os.getpid()))\nwhile not os.path.isfile(os.path.join(os.path.dirname(__file__), 'release')):\n    time.sleep(0.002)\nsys.stdout.buffer.write({EMPTY})"
            ));
            let next = Helper::new(&format!(
                "sys.stdin.buffer.read()\nsys.stdout.buffer.write({EMPTY})"
            ));
            let checks = AtomicU64::new(0);
            thread::scope(|scope| {
                let first_run = scope.spawn(|| {
                    portable::run(
                        first.command(),
                        vec![],
                        Instant::now() + Duration::from_secs(5),
                        &|| false,
                    )
                });
                let started = first.0.parent().unwrap().join("pid");
                let ready_deadline = Instant::now() + Duration::from_secs(2);
                while !started.is_file() && Instant::now() < ready_deadline {
                    pause(ready_deadline);
                }
                assert!(started.is_file(), "first helper must hold the active lease");
                let began = Instant::now();
                let cancelled = portable::run(
                    next.command(),
                    vec![],
                    began + Duration::from_secs(2),
                    &|| began.elapsed() >= Duration::from_millis(20),
                );
                let expired = portable::run(
                    next.command(),
                    vec![],
                    Instant::now() + Duration::from_millis(20),
                    &|| false,
                );
                let second_run = scope.spawn(|| {
                    portable::run(
                        next.command(),
                        vec![],
                        Instant::now() + Duration::from_secs(5),
                        &|| {
                            checks.fetch_add(1, Ordering::Relaxed);
                            false
                        },
                    )
                });
                let waiting_deadline = Instant::now() + Duration::from_secs(2);
                while checks.load(Ordering::Relaxed) < 2
                    && !second_run.is_finished()
                    && Instant::now() < waiting_deadline
                {
                    pause(waiting_deadline);
                }
                fs::write(first.0.parent().unwrap().join("release"), []).unwrap();
                assert!(GenerationResponse::decode(&first_run.join().unwrap().unwrap()).is_ok());
                assert!(GenerationResponse::decode(&second_run.join().unwrap().unwrap()).is_ok());
                assert!(matches!(cancelled, Err(GenerationError::Cancelled)));
                assert!(matches!(expired, Err(GenerationError::Deadline)));
            });
        }

        #[test]
        fn portable_workers_finish_without_blocking_joins() {
            let _serial = HELPER_TEST.lock().unwrap();
            let helper = Helper::new(&format!(
                "sys.stdin.buffer.read()\nsys.stdout.buffer.write({EMPTY})"
            ));
            let bytes = portable::run(
                helper.command(),
                vec![1; 262_144],
                Instant::now() + Duration::from_secs(5),
                &|| false,
            )
            .unwrap();
            assert!(GenerationResponse::decode(&bytes).is_ok());

            let inherited = Helper::new("with open(os.path.join(os.path.dirname(__file__), 'pid'), 'w') as f:\n    f.write(str(os.getpid()))\npid = os.fork()\nif pid == 0:\n    time.sleep(0.5)\nelse:\n    os._exit(0)");
            let began = Instant::now();
            assert!(matches!(
                portable::run(
                    inherited.command(),
                    vec![],
                    began + Duration::from_millis(120),
                    &|| false
                ),
                Err(GenerationError::Deadline)
            ));
            assert!(began.elapsed() < Duration::from_secs(1));
            inherited.assert_reaped();
            assert!(matches!(
                portable::run(
                    helper.command(),
                    vec![],
                    Instant::now() + Duration::from_secs(1),
                    &|| false
                ),
                Err(GenerationError::Helper)
            ));
            // The finite mock descendant closes its handles; the lease then recovers.
            thread::sleep(Duration::from_millis(500));
            let bytes = portable::run(
                helper.command(),
                vec![],
                Instant::now() + Duration::from_secs(2),
                &|| false,
            )
            .unwrap();
            assert!(GenerationResponse::decode(&bytes).is_ok());
        }
    }
}
