# Roadmap to `std` parity

purestd is a drop-in `std` on raw syscalls. "Parity" means: take a real-world
Rust program that uses `std`, alias `purestd` as `std`, and have it compile and
behave the same — for the parts of `std` that program touches.

The bulk of this roadmap is **done** and exercised in CI on macOS arm64, Linux
x86_64, and Linux aarch64 (build + run, every push). This document now records
what's implemented and the short list that remains.

## Status legend

- ✅ done — usable, behaves like `std`
- 🟡 partial / accepted divergence
- ⬜ not yet
- ⛔ non-goal (see [Non-goals](#non-goals))

## Module status

| Module | Status | Notes |
| --- | --- | --- |
| `core`/`alloc` re-exports | ✅ | mirrored under `std::…` |
| `io` | ✅ | `Read`/`Write`/`Seek`/`BufRead`, vectored I/O (`IoSlice`/`read_vectored`/`write_vectored` via `readv`/`writev`), `Error`/`ErrorKind`, `BufReader`/`BufWriter`/`Cursor`/`copy`/`Lines`, `Stdin`/`Stdout`/`Stderr` + locks, `read_line`, `io::prelude` |
| `fs` | ✅ | `File`/`OpenOptions`, read/write/copy/rename, `Metadata`/`FileType`/`Permissions`, `read_dir`/`DirEntry`, `seek`/`set_len`/`sync_all`, create/remove dir (+recursive) |
| `env` | ✅ | `args`, `var`/`set_var`/`remove_var`/`vars` (mutable), `current_dir`/`set_current_dir`, `temp_dir` |
| `process` | ✅ | `exit`/`abort`/`id`/`ExitCode`; `Command`/`Child`/`Stdio`/`Output`/`ExitStatus` (fork+execve, pipes, PATH search) |
| `time` | ✅ | `Duration`; **monotonic** `Instant` (CLOCK_MONOTONIC / CNTVCT); `SystemTime`; arithmetic |
| `sync` | ✅ | futex `Mutex`, `RwLock` (incl. `try_read`/`try_write`), `Condvar`, `Barrier`, `LazyLock`, `Once`/`OnceLock`, `mpsc`; `Arc`, `atomic` |
| `path` | ✅ | `Path`/`PathBuf`, `file_name`/`file_stem`/`extension`/`parent`, `join`/`push`/`pop`, `starts_with`/`strip_prefix`, `with_extension`, `components`/`ancestors`, `display`, `exists`/`is_file`/`is_dir` |
| `ffi` | 🟡 | `CStr`/`CString`, `OsStr`/`OsString` — UTF-8, not raw bytes (see below) |
| `error` | ✅ | `core::error::Error` |
| `collections` | ✅ | own `HashMap`/`HashSet` (+ `retain`/set-ops/entry) + the `alloc` containers |
| `hash` | ✅ | SipHash-1-3 + `RandomState`, verified vs reference vectors |
| `thread` | ✅ | `spawn`/`join`/`Builder`, `sleep`/`yield_now`, `scope`, `available_parallelism`, `current()` (real tid). No `park`/`unpark` |
| `net` | ✅ | TCP/UDP, `ToSocketAddrs`, DNS (`/etc/hosts` + plain DNS); socket options/timeouts/`try_clone`/`peek` |
| `os::fd` / `os::unix` | ✅ | `RawFd`/`OwnedFd`/`AsRawFd`/…; `MetadataExt`/`PermissionsExt`/`OsStrExt` |
| `tls` (`thread_local!`) | ✅ | key-based; no destructors at thread exit yet |
| `panic` | ✅ | `#[panic_handler]`, `catch_unwind`/`resume_unwind`/`AssertUnwindSafe` (abort model) |
| `backtrace` | 🟡 | API present; capture is `Unsupported` (no unwinder) |

## What remains

A short list — none of it blocks the common path:

- **`thread::park`/`unpark`** and OS-level thread names. (TLS is in place, so the
  parker can hang off the current thread now.)
- **TLS destructors** at thread exit (values currently leak when a thread ends).
- **DNS hardening:** multiple nameservers + retries/timeouts, `search` domains,
  CNAME chains, TCP fallback on truncation.
- **Platform breadth:** macOS x86_64, more Linux arches (riscv64/arm), *BSD.

## Accepted divergences

- **`OsStr`/`Path` are UTF-8, not raw OS bytes.** Real `std` keeps arbitrary
  bytes; we use UTF-8 (lossy at the boundary). Non-UTF-8 filenames are the only
  thing affected. Revisit only if a real consumer needs it.
- **`panic = "abort"` only.** No stack unwinding, so `catch_unwind` never returns
  `Err` and `Backtrace` is `Unsupported`. Most freestanding users want `abort`;
  full DWARF unwinding is a large effort we don't plan unless needed.
- **No NSS.** Name resolution is `/etc/hosts` + plain DNS forever — no
  `nsswitch`/mDNS/LDAP. Intentional ("no foreign code").

## Non-goals

- **Windows** and other non-Unix OSes.
- **NSS / `getaddrinfo` plugin behavior.**
- **Dynamic linking / FFI into `.so`/`.dylib`** (purestd targets fully-static,
  foreign-code-free binaries).
- **An async runtime** (`std` doesn't ship one either).

## How parity is exercised

CI builds and runs a corpus of examples on macOS arm64 and Linux x86_64/aarch64
every push: io/fs/env/process/time/sync/threads/scope/tls/net/collections, plus
purity (no libc calls) and static-link assertions on the fullrust side. New gaps
surface as `not found in std` errors or behavioral asserts and become the next
items above.
