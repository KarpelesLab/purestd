# Roadmap to `std` parity

purestd aims to be a drop-in `std` on raw syscalls. "Parity" here means: take a
real-world Rust program that uses `std`, alias `purestd` as `std`, and have it
compile and behave the same — for the parts of `std` that program actually
touches. We chase the **common path first**, then fidelity and edge cases.

This document tracks where we are and the order we'll close the gap.

## Status legend

- ✅ done — usable, behaves like `std`
- 🟡 partial — exists but missing methods / fidelity caveats
- ⬜ missing
- ⛔ non-goal (see [Non-goals](#non-goals))

## Where we are today

| Module | Status | Notes |
| --- | --- | --- |
| `core` / `alloc` re-exports | ✅ | mirrored under `std::…` paths |
| `io` | 🟡 | `Read`/`Write`/`Error`/`ErrorKind`/`Result`, `Stdin/out/err`. No `Seek`, `BufRead`, `BufReader`/`BufWriter`, `Cursor`, stdio locking, vectored I/O |
| `fs` | 🟡 | `File`, `OpenOptions`, `read`/`read_to_string`/`write`/`remove_file`/`create_dir`. No metadata, `read_dir`, `seek`, `rename`/`copy`, permissions |
| `env` | 🟡 | `args`/`vars`/`var`. No `set_var`/`current_dir`/`temp_dir`/`*_os` |
| `process` | 🟡 | `exit`/`abort`/`id`/`ExitCode`. No `Command`/`Child`/`Stdio` |
| `time` | 🟡 | `Duration`/`Instant`/`SystemTime`. **`Instant` is wall-clock, not monotonic** |
| `sync` | 🟡 | `Mutex`/`RwLock`/`Once`/`OnceLock` (spin), `Arc`, `atomic`. No `Condvar`/`Barrier`/`mpsc`/`LazyLock`; locks should be futex-backed |
| `path` | 🟡 | `Path`/`PathBuf` basics. UTF-8 only; missing `components`/`strip_prefix`/`Display`/… |
| `ffi` | 🟡 | `CStr`/`CString`, `OsStr`/`OsString`. `OsStr` is UTF-8, not raw bytes |
| `error` | ✅ | `core::error::Error` |
| `collections` | 🟡 | own `HashMap`/`HashSet` + `alloc` containers. Missing some methods (`retain`/`drain`/set-ops) |
| `hash` | ✅ | SipHash-1-3 + `RandomState` (verified vs reference vectors) |
| `thread` | 🟡 | `spawn`/`join`/`Builder`/`sleep`/`yield_now`. No `park`/names/scope/`available_parallelism`; no TLS |
| `net` | 🟡 | TCP/UDP/`ToSocketAddrs` + DNS. No socket options/timeouts/`try_clone` |
| runtime | ✅ | `entry!`, `#[panic_handler]`, `#[global_allocator]`, `rust_eh_personality` |
| `os` (`os::fd`, `os::unix`) | ⬜ | no `RawFd`/`AsRawFd`/`OwnedFd`, no unix extension traits |
| `thread_local!` / TLS | ⬜ | not implemented |
| `panic::catch_unwind` | ⛔/⬜ | we build `panic = "abort"`; see [Hard problems](#hard-problems--decisions) |

Targets today: `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu` — all build-and-run in CI.

## Phase 0 — Foundations & correctness

These are either correctness fixes or unblock many later APIs.

- [ ] **Monotonic `Instant`.** Switch `Instant` off `gettimeofday` to
      `clock_gettime(CLOCK_MONOTONIC)` (Linux) / a monotonic source on macOS.
      Today `Instant` can go backwards — a real bug.
- [ ] **`io::Seek` + `File::seek`** (have the `lseek` syscall numbers already).
- [ ] **Buffered I/O:** `BufReader`, `BufWriter`, `BufRead` (+ `lines`,
      `read_line`, `read_until`), `io::Cursor`, `io::copy`, `Write for Vec<u8>`.
- [ ] **Standard-stream fidelity:** line-buffered `Stdout`, `Stdin::lock`/
      `read_line`, `StdoutLock`/`StderrLock`, `Stdout::flush` on exit.
- [ ] **`os::fd`:** `RawFd`, `AsRawFd`/`AsFd`, `FromRawFd`/`IntoRawFd`,
      `OwnedFd`/`BorrowedFd`. Re-base `File`/`TcpStream`/… on `OwnedFd`.
- [ ] **`OsStr`/`OsString`/`Path` as raw bytes** (platform-correct), keeping the
      UTF-8 helpers. Add `os::unix::ffi::OsStrExt`.
- [ ] **`io::Error` completeness:** full `errno → ErrorKind` table,
      `last_os_error`, `Error::other`, `raw_os_error` everywhere.

## Phase 1 — Everyday APIs

- [ ] **`fs`:** `metadata`/`symlink_metadata`/`Metadata`/`FileType`/
      `Permissions`, `read_dir`/`DirEntry`, `create_dir_all`/`remove_dir`/
      `remove_dir_all`, `rename`, `copy`, `canonicalize`, `File::set_len`/
      `sync_all`/`sync_data`, `OpenOptions` full set.
- [ ] **`env`:** `set_var`/`remove_var`, `current_dir`/`set_current_dir`,
      `temp_dir`, `args_os`/`vars_os`/`var_os`, `current_exe`.
- [ ] **`sync` (futex-backed):** replace spinlocks with futex-blocking
      `Mutex`/`RwLock`; add `Condvar`, `Barrier`, `LazyLock`, and `mpsc`
      (`channel`/`sync_channel`).
- [ ] **`thread`:** `park`/`unpark`, `Thread::set_name`/`name` (prctl /
      `pthread_setname` raw), `available_parallelism`
      (`sched_getaffinity`/`sysctl`), `thread::scope` (scoped threads).
- [ ] **`net` options:** `set_nodelay`, `set_read_timeout`/`set_write_timeout`
      (`SO_RCVTIMEO`), `set_nonblocking`, `set_ttl`, `try_clone`, `peek`;
      `UdpSocket` `broadcast`/`multicast`/`set_broadcast`.

## Phase 2 — Bigger subsystems

- [ ] **TLS: `thread_local!`.** Real per-thread storage (TLS block set up at
      thread creation; `__tls_get_addr` / `#[thread_local]` or key-based). Many
      libraries assume this; it also unblocks the full thread test suite.
- [ ] **`process::Command`.** `Command`/`Child`/`Stdio`/`Output`/`ExitStatus`
      via `fork`+`execve` (or `posix_spawn`/`vfork`), pipes, `wait4`,
      env/arg/cwd/`pre_exec`.
- [ ] **Vectored & misc I/O:** `readv`/`writev` (`IoSlice`/`IoSliceMut`),
      `Read::bytes`, `Write::write_fmt` parity, `BufRead::split`.
- [ ] **`path` completeness:** `components`/`ancestors`/`strip_prefix`/
      `with_extension`/`with_file_name`/`Display`/`Path::exists`/`is_file`/
      `is_dir` (via `fs::metadata`).

## Phase 3 — Completeness & fidelity

- [ ] **`panic::catch_unwind`** — see [Hard problems](#hard-problems--decisions).
- [ ] **`collections`** method parity: `retain`/`drain`/`get_or_insert_with`,
      `HashSet` set algebra (`union`/`intersection`/…), `Entry` completeness,
      `hash_map`/`hash_set` submodule surface.
- [ ] **DNS hardening:** multiple nameservers + retries/timeouts, `search`
      domains, CNAME chains, TCP fallback on truncation, AAAA + happy-eyeballs
      address ordering.
- [ ] **`time`:** `SystemTime` ± `Duration`, `checked_add`/`checked_sub`,
      higher-resolution clocks.
- [ ] **`os::unix` extensions:** `fs::PermissionsExt`/`MetadataExt`,
      `process::CommandExt`, `net` ext, `thread::JoinHandleExt`.
- [ ] **`Backtrace`** — likely a no-op/`Unsupported` stub (needs an unwinder +
      symbolication); document the limitation.

## Phase 4 — Platform breadth

- [ ] `x86_64-apple-darwin` (macOS Intel).
- [ ] More Linux arches: `riscv64`, `arm`, `x86`.
- [ ] *BSD (FreeBSD/OpenBSD) — syscall ABIs differ but are stable.
- Windows is a [non-goal](#non-goals).

## Hard problems / decisions

- **Unwinding vs `panic = "abort"`.** We build `abort`, so there is no real
  stack unwinding and `catch_unwind` can't actually catch. Options: (a) keep
  `abort` and provide a `catch_unwind` that documents it never returns `Err`
  (current direction); (b) implement DWARF unwinding + a personality routine to
  support `panic = "unwind"`. (b) is a large effort and pulls in unwind tables;
  most freestanding users want `abort`. **Leaning (a)**, revisit if a real
  consumer needs unwinding.
- **`OsStr` as bytes vs UTF-8.** Correct `std` keeps raw OS bytes. We started
  UTF-8 for simplicity; Phase 0 moves to raw bytes with `OsStrExt`, which is a
  small breaking change to `ffi`/`path` internals.
- **Monotonic clock on macOS.** `mach_absolute_time` is a commpage read, not a
  syscall; `clock_gettime` is available but historically libc-mediated. Need to
  pick a pure-syscall monotonic source on Darwin.
- **Futex-backed locks.** Spinlocks are correct but burn CPU under contention.
  Moving to futex (`__ulock` on macOS, `futex` on Linux) blocking is Phase 1 and
  shared with the existing join machinery.
- **No NSS.** Name resolution is `/etc/hosts` + plain DNS forever — no
  `nsswitch`/mDNS/LDAP. This is intentional (matches the "no foreign code"
  stance) and is a permanent, documented divergence.

## Non-goals

- **Windows** (and other non-Unix OSes) — a fundamentally different model.
- **NSS / `getaddrinfo` plugin behavior** — see above.
- **Dynamic linking / FFI into `.so`/`.dylib`** — purestd targets fully-static,
  foreign-code-free binaries.
- **An async runtime** — `std` doesn't ship one either.

## Measuring parity

The intended conformance method: maintain a corpus of small, real `std`
programs, build each with purestd aliased as `std` (via the fullrust toolchain),
and run them. Every gap shows up as an ordinary `not found in std` /
`unresolved import` error or a behavioral assert — those become the next items
above. CI already exercises io/fs/env/process/time/sync/threads/net/collections
on macOS arm64 and Linux x86_64/aarch64 on every push.
