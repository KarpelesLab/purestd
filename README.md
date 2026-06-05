# purestd

A **drop-in replacement for Rust's `std` that does not depend on libc.** Every
operation is a direct kernel syscall — no C library, no C runtime. Built on
`core` + `alloc`, purestd supplies exactly what a real `std` provides (panic
handler, global allocator, I/O, the `std`-shaped API) and keeps Rust's
guarantees intact all the way down to the syscall instruction.

The pieces a libc-free binary *also* needs but which in a hosted build come from
**crt0 / compiler_builtins / the unwinder** — the process entry point `_start`,
the `mem*`/unwind/`getauxval` symbols — are *not* in purestd. They live in a tiny
companion crate, [`purert`](crt/), which a program links alongside purestd
(`extern crate purert;`). So purestd is only `std`; `purert` is only the runtime.

It is designed to be [fullrust](https://github.com/KarpelesLab/fullrust)'s
standard library — programs written against purestd compile, via the fullrust
toolchain, into real fully-static libc-free binaries — and to be usable on its
own.

**Zero third-party dependencies.** purestd is built on `core` + `alloc` and
nothing else — it implements its own hash map, hasher, allocator, and OS layer.
`cargo tree` shows only `purestd`.

```rust
#![no_std]
#![no_main]

extern crate purert; // the runtime: _start + the mem*/unwind symbols
use purestd::prelude::*;

fn main() {
    println!("hello from purestd — no libc");
}

purestd::entry!(main);
```

## "No libc" — and how it's verified

* **Linux** binaries are fully static ELFs with **no dynamic interpreter and no
  libc** at all (`file` reports `statically linked`, no `/lib64/ld`).
* **macOS** links `libSystem` as a load-command only because `ld64` mandates it;
  purestd makes **zero calls into it**. `nm -u <binary>` lists only the loader
  stub `dyld_stub_binder`. The `memcpy`/`memset`/`bzero`/`strlen` it references
  are defined by `purert` (see [`crt/src/intrinsics.rs`](crt/src/intrinsics.rs)).

## Targets

| Target                         | Status                                   |
| ------------------------------ | ---------------------------------------- |
| `aarch64-apple-darwin`         | builds & **runs natively** (dev loop)    |
| `x86_64-unknown-linux-gnu`     | builds → fully-static libc-free ELF      |
| `aarch64-unknown-linux-gnu`    | builds → fully-static libc-free ELF      |

The only architecture/OS-specific code lives in [`src/arch/`](src/arch/): one
file per target, providing the raw `syscallN` wrappers, the entry point, and the
syscall number table. Everything above is OS-neutral.

## Layout

```
purestd (the std):
  arch/      raw syscall wrappers + number table (per target)
  syscall    arch-neutral, Result-returning wrappers (Errno)
  allocator  mmap-backed segregated free-list (#[global_allocator])
  panic      the #[panic_handler]
  start      __purestd_start — the lang_start-equivalent runtime glue
  rt         exit/abort + Termination (main may return (), i32, Result)
  io fs env process time sync path ffi error net thread   ← the std surface

purert (the runtime — crt0/compiler_builtins equivalent):
  entry      _start (per target) → __purestd_start
  intrinsics mem*/strlen + unwind stubs + getauxval
```

`core` and `alloc` are re-exported under `std`-shaped paths (`mem`, `cmp`,
`fmt`, `vec`, `collections`, …), so when purestd is aliased as `std` for a
freestanding target, ordinary `use std::io::Write;` / `std::fs::read(..)` /
`HashMap` code resolves here unchanged.

## The `rt` feature (default on)

Gates the std-provided *policy* symbols — the `#[panic_handler]`, the
`#[global_allocator]` static, and the `lang_start`-equivalent runtime glue
(`__purestd_start`). Disable it (`default-features = false`) when a host runtime
supplies those instead. The *mechanisms* (syscalls, the allocator type, the
`std` surface) are always available. The process entry point and the
`mem*`/unwind intrinsics are not gated here — they are simply the `purert` crate.

## Building

Native (macOS, runs immediately):

```sh
cargo run --example stdshow -- some args
```

Fully-static libc-free Linux ELF (cross-compiled from macOS via rust-lld):

```sh
scripts/build-linux.sh stdshow x86_64
scripts/build-linux.sh stdshow aarch64
```

## Status

Working: process entry/exit, panic, mmap allocator, `args`/env, `io` (`Read`/
`Write`/`Error`/std streams), `fs` (`File`/`OpenOptions`/`read`/`write`),
`process`, `time`, `sync` (`Mutex`/`RwLock`/`Once`/`OnceLock`), `path`, `ffi`,
`hash` (own SipHash-1-3 + `RandomState` seeded from `getrandom`/`getentropy`;
verified against the canonical reference vectors — `cargo run --example sipcheck`),
`collections` (own open-addressing `HashMap`/`HashSet` + the `alloc` containers).

Threads are real OS threads: `thread::spawn`/`JoinHandle::join`/`Builder`,
`sleep`, `yield_now`, futex-based join. On macOS they use Mach
`thread_create_running` (no libc, no libpthread — see `docs/macos-threads.md`)
and run natively; on Linux they use `clone` + `futex` (compile-verified).

Not yet: `process::Command`, sockets (`net`), buffered readers/writers, TLS,
thread names/parking. `net` is a compiling placeholder.

## License

MIT OR Apache-2.0.
