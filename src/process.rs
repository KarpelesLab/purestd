//! `std::process` subset: `exit`, `abort`, `id`, and `ExitCode`.
//!
//! `Command` (subprocess spawning) is not implemented yet — it needs
//! `fork`/`posix_spawn`/`execve` wiring and lands later.

use crate::syscall;

/// Terminate the current process with the given exit code. Drop-in for
/// `std::process::exit`.
#[inline]
pub fn exit(code: i32) -> ! {
    crate::rt::exit(code)
}

/// Terminate abnormally (status 134). Drop-in for `std::process::abort`.
#[inline]
pub fn abort() -> ! {
    crate::rt::abort()
}

/// The id of the current process. Drop-in for `std::process::id`.
#[inline]
pub fn id() -> u32 {
    syscall::getpid()
}

/// A process exit code. Drop-in for `std::process::ExitCode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitCode(u8);

impl ExitCode {
    pub const SUCCESS: ExitCode = ExitCode(0);
    pub const FAILURE: ExitCode = ExitCode(1);

    pub fn exit_process(self) -> ! {
        exit(self.0 as i32)
    }
}

impl From<u8> for ExitCode {
    fn from(n: u8) -> ExitCode {
        ExitCode(n)
    }
}

impl crate::rt::Termination for ExitCode {
    fn report(self) -> i32 {
        self.0 as i32
    }
}

// ---------------------------------------------------------------------------
// Command / Child / Stdio (fork + execve)
// ---------------------------------------------------------------------------

use crate::alloc::collections::BTreeMap;
use crate::alloc::string::String;
use crate::alloc::vec::Vec;
use crate::ffi::CString;
use crate::io::{self, Error, ErrorKind, Read, Write};

#[derive(Clone, Copy, PartialEq, Eq)]
enum StdioKind {
    Inherit,
    Null,
    Piped,
}

/// Describes a stdio stream for a child. Drop-in for `std::process::Stdio`.
pub struct Stdio(StdioKind);
impl Stdio {
    pub fn inherit() -> Stdio {
        Stdio(StdioKind::Inherit)
    }
    pub fn null() -> Stdio {
        Stdio(StdioKind::Null)
    }
    pub fn piped() -> Stdio {
        Stdio(StdioKind::Piped)
    }
}

/// A process builder. Drop-in for `std::process::Command`.
pub struct Command {
    program: String,
    args: Vec<String>,
    env_ops: BTreeMap<String, Option<String>>,
    env_clear: bool,
    cwd: Option<String>,
    stdin: StdioKind,
    stdout: StdioKind,
    stderr: StdioKind,
}

impl Command {
    pub fn new<S: AsRef<str>>(program: S) -> Command {
        Command {
            program: program.as_ref().into(),
            args: Vec::new(),
            env_ops: BTreeMap::new(),
            env_clear: false,
            cwd: None,
            stdin: StdioKind::Inherit,
            stdout: StdioKind::Inherit,
            stderr: StdioKind::Inherit,
        }
    }
    pub fn arg<S: AsRef<str>>(&mut self, arg: S) -> &mut Command {
        self.args.push(arg.as_ref().into());
        self
    }
    pub fn args<I: IntoIterator<Item = S>, S: AsRef<str>>(&mut self, args: I) -> &mut Command {
        for a in args {
            self.args.push(a.as_ref().into());
        }
        self
    }
    pub fn env<K: AsRef<str>, V: AsRef<str>>(&mut self, key: K, val: V) -> &mut Command {
        self.env_ops
            .insert(key.as_ref().into(), Some(val.as_ref().into()));
        self
    }
    pub fn env_remove<K: AsRef<str>>(&mut self, key: K) -> &mut Command {
        self.env_ops.insert(key.as_ref().into(), None);
        self
    }
    pub fn env_clear(&mut self) -> &mut Command {
        self.env_clear = true;
        self
    }
    pub fn current_dir<P: AsRef<crate::path::Path>>(&mut self, dir: P) -> &mut Command {
        self.cwd = Some(dir.as_ref().as_str().into());
        self
    }
    pub fn stdin(&mut self, cfg: Stdio) -> &mut Command {
        self.stdin = cfg.0;
        self
    }
    pub fn stdout(&mut self, cfg: Stdio) -> &mut Command {
        self.stdout = cfg.0;
        self
    }
    pub fn stderr(&mut self, cfg: Stdio) -> &mut Command {
        self.stderr = cfg.0;
        self
    }

    fn build_env(&self) -> Vec<CString> {
        let mut map: BTreeMap<String, String> = if self.env_clear {
            BTreeMap::new()
        } else {
            crate::env::vars().collect()
        };
        for (k, v) in &self.env_ops {
            match v {
                Some(val) => {
                    map.insert(k.clone(), val.clone());
                }
                None => {
                    map.remove(k);
                }
            }
        }
        map.iter()
            .filter_map(|(k, v)| {
                let mut s = String::with_capacity(k.len() + v.len() + 1);
                s.push_str(k);
                s.push('=');
                s.push_str(v);
                CString::new(s.into_bytes()).ok()
            })
            .collect()
    }

    pub fn spawn(&mut self) -> io::Result<Child> {
        // Resolve the program path (search $PATH if it has no slash).
        let prog_path = resolve_program(&self.program)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "program not found"))?;
        let prog = CString::new(prog_path.into_bytes())
            .map_err(|_| Error::from(ErrorKind::InvalidInput))?;

        // argv: [program, args..]
        let mut argv_owned: Vec<CString> = Vec::with_capacity(self.args.len() + 1);
        argv_owned.push(CString::new(self.program.clone().into_bytes()).unwrap());
        for a in &self.args {
            argv_owned.push(
                CString::new(a.clone().into_bytes()).map_err(|_| Error::from(ErrorKind::InvalidInput))?,
            );
        }
        let argv: Vec<*const u8> = argv_owned
            .iter()
            .map(|c| c.as_ptr() as *const u8)
            .chain(core::iter::once(core::ptr::null()))
            .collect();

        let env_owned = self.build_env();
        let envp: Vec<*const u8> = env_owned
            .iter()
            .map(|c| c.as_ptr() as *const u8)
            .chain(core::iter::once(core::ptr::null()))
            .collect();

        let cwd_c = match &self.cwd {
            Some(d) => Some(CString::new(d.clone().into_bytes()).unwrap()),
            None => None,
        };

        // Set up pipes (parent_fd, child_fd) for any Piped stream.
        let stdin_pipe = make_pipe(self.stdin, true)?;
        let stdout_pipe = make_pipe(self.stdout, false)?;
        let stderr_pipe = make_pipe(self.stderr, false)?;

        let pid = crate::syscall::fork().map_err(Error::from)?;
        if pid == 0 {
            // ---- child: only syscalls, no allocation ----
            if let Some(c) = &cwd_c {
                let _ = crate::syscall::chdir(c);
            }
            child_setup_fd(0, self.stdin, &stdin_pipe);
            child_setup_fd(1, self.stdout, &stdout_pipe);
            child_setup_fd(2, self.stderr, &stderr_pipe);
            let _ = crate::syscall::execve(&prog, argv.as_ptr(), envp.as_ptr());
            crate::rt::exit(127); // exec failed
        }

        // ---- parent ----
        // Close the child ends; keep the parent ends.
        let stdin = parent_keep(self.stdin, &stdin_pipe, true);
        let stdout = parent_keep(self.stdout, &stdout_pipe, false);
        let stderr = parent_keep(self.stderr, &stderr_pipe, false);

        Ok(Child {
            pid,
            stdin: stdin.map(ChildStdin),
            stdout: stdout.map(ChildStdout),
            stderr: stderr.map(ChildStderr),
        })
    }

    pub fn status(&mut self) -> io::Result<ExitStatus> {
        self.spawn()?.wait()
    }

    pub fn output(&mut self) -> io::Result<Output> {
        self.stdout(Stdio::piped());
        self.stderr(Stdio::piped());
        let mut child = self.spawn()?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut o) = child.stdout.take() {
            o.read_to_end(&mut stdout)?;
        }
        if let Some(mut e) = child.stderr.take() {
            e.read_to_end(&mut stderr)?;
        }
        let status = child.wait()?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

// A pipe pair: parent end and child end (or None if not Piped).
struct PipePair {
    parent: i32,
    child: i32,
}

fn make_pipe(kind: StdioKind, is_stdin: bool) -> io::Result<Option<PipePair>> {
    if kind != StdioKind::Piped {
        return Ok(None);
    }
    let (r, w) = crate::syscall::pipe().map_err(Error::from)?;
    // stdin: child reads (r), parent writes (w). stdout/err: child writes (w),
    // parent reads (r).
    Ok(Some(if is_stdin {
        PipePair {
            parent: w,
            child: r,
        }
    } else {
        PipePair {
            parent: r,
            child: w,
        }
    }))
}

fn child_setup_fd(target: i32, kind: StdioKind, pipe: &Option<PipePair>) {
    match kind {
        StdioKind::Inherit => {}
        StdioKind::Null => {
            if let Ok(devnull) = open_devnull() {
                let _ = crate::syscall::dup2(devnull, target);
                let _ = crate::syscall::close(devnull);
            }
        }
        StdioKind::Piped => {
            if let Some(p) = pipe {
                let _ = crate::syscall::dup2(p.child, target);
                let _ = crate::syscall::close(p.child);
                let _ = crate::syscall::close(p.parent);
            }
        }
    }
}

fn parent_keep(kind: StdioKind, pipe: &Option<PipePair>, _is_stdin: bool) -> Option<i32> {
    if kind == StdioKind::Piped {
        if let Some(p) = pipe {
            let _ = crate::syscall::close(p.child);
            return Some(p.parent);
        }
    }
    None
}

fn open_devnull() -> Result<i32, crate::syscall::Errno> {
    let c = c"/dev/null";
    crate::syscall::open(c, crate::syscall::O_RDWR, 0)
}

fn resolve_program(program: &str) -> Option<String> {
    if program.contains('/') {
        return Some(program.into());
    }
    let path = crate::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into());
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = if dir.ends_with('/') {
            crate::alloc::format!("{dir}{program}")
        } else {
            crate::alloc::format!("{dir}/{program}")
        };
        if crate::fs::metadata(&candidate).map(|m| m.is_file()).unwrap_or(false) {
            return Some(candidate);
        }
    }
    None
}

/// A spawned child process. Drop-in for `std::process::Child`.
pub struct Child {
    pub pid: i32,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

impl Child {
    pub fn id(&self) -> u32 {
        self.pid as u32
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        // Ensure stdin is closed so the child can finish.
        self.stdin = None;
        let mut status = 0i32;
        loop {
            match crate::syscall::wait4(self.pid, &mut status, 0) {
                Ok(_) => return Ok(ExitStatus(status)),
                Err(crate::syscall::Errno(4)) => continue, // EINTR
                Err(e) => return Err(Error::from(e)),
            }
        }
    }
    pub fn wait_with_output(mut self) -> io::Result<Output> {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut o) = self.stdout.take() {
            o.read_to_end(&mut stdout)?;
        }
        if let Some(mut e) = self.stderr.take() {
            e.read_to_end(&mut stderr)?;
        }
        let status = self.wait()?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

macro_rules! child_stream {
    ($name:ident, $read:expr, $write:expr) => {
        pub struct $name(i32);
        impl Drop for $name {
            fn drop(&mut self) {
                let _ = crate::syscall::close(self.0);
            }
        }
    };
}
child_stream!(ChildStdin, false, true);
child_stream!(ChildStdout, true, false);
child_stream!(ChildStderr, true, false);

impl Write for ChildStdin {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Fd(self.0).write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Read for ChildStdout {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Fd(self.0).read(buf)
    }
}
impl Read for ChildStderr {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Fd(self.0).read(buf)
    }
}

/// The status of a finished process. Drop-in for `std::process::ExitStatus`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ExitStatus(i32);
impl ExitStatus {
    pub fn success(&self) -> bool {
        self.code() == Some(0)
    }
    pub fn code(&self) -> Option<i32> {
        // WIFEXITED: low 7 bits == 0. WEXITSTATUS: bits 8..16.
        if self.0 & 0x7f == 0 {
            Some((self.0 >> 8) & 0xff)
        } else {
            None
        }
    }
}

/// The captured output of a finished process. Drop-in for `std::process::Output`.
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
