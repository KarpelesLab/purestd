# purestd

A **drop-in replacement for Rust's `std` that does not depend on libc.** Every
operation is a direct kernel syscall — no C library, no C runtime. Built on
`core` + `alloc`, purestd supplies the layer a freestanding program is otherwise
missing (process entry, panic handler, allocator, I/O, the `std`-shaped API) and
keeps Rust's guarantees intact all the way down to the syscall instruction.

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
  stub `dyld_stub_binder`. Our own `memcpy`/`memset`/`bzero`/`strlen` are defined
  locally (see [`src/intrinsics.rs`](src/intrinsics.rs)).

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
arch/      raw syscall wrappers + number table + _start (per target)
syscall    arch-neutral, Result-returning wrappers (Errno)
allocator  mmap-backed segregated free-list (#[global_allocator])
intrinsics mem*/strlen + unwind stubs the toolchain needs without libc
rt         exit/abort + Termination (main may return (), i32, Result)
io fs env process time sync path ffi error net thread   ← the std surface
```

`core` and `alloc` are re-exported under `std`-shaped paths (`mem`, `cmp`,
`fmt`, `vec`, `collections`, …), so when purestd is aliased as `std` for a
freestanding target, ordinary `use std::io::Write;` / `std::fs::read(..)` /
`HashMap` code resolves here unchanged.

## The `rt` feature (default on)

Gates the binary-level *policy* symbols — `_start`, the `#[panic_handler]`, the
`#[global_allocator]` static, and the `mem*`/unwind intrinsics. Disable it
(`default-features = false`) when a host runtime supplies those instead. The
*mechanisms* (syscalls, the allocator type, the `std` surface) are always
available.

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
`hash` (own SipHash-1-3 + `RandomState` seeded from `getrandom`/`getentropy`),
`collections` (own open-addressing `HashMap`/`HashSet` + the `alloc` containers).

Not yet: real threads (clone/futex), `process::Command`, sockets (`net`),
buffered readers/writers. `thread`/`net` are present as compiling placeholders.

## License

MIT OR Apache-2.0.
