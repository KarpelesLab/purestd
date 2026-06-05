# purestd

A **`std` written in pure Rust that never calls libc.** Every operation —
files, time, env, threads, the allocator — is a direct kernel syscall
(`svc`/`syscall`). Built on `core` + `alloc`, purestd provides exactly what a
real `std` provides (the panic handler, the global allocator, I/O, the
`std`-shaped API) and keeps Rust's guarantees intact all the way down to the
syscall instruction.

purestd is *only* `std`. The process entry point (`_start`) and the
`mem*`/unwind symbols are not its concern — in a normal build they come from
crt0 and libc, and in a libc-free build from the
[fullrust](https://github.com/KarpelesLab/fullrust) toolchain. So purestd builds
and runs fine as an ordinary `no_std` library **with** libc present (which is how
it's tested here); it simply never *calls* into libc.

**Zero third-party dependencies.** purestd is built on `core` + `alloc` and
nothing else — it implements its own hash map, hasher, allocator, and OS layer.
`cargo tree` shows only `purestd`.

```rust
#![no_std]
#![no_main]

use purestd::prelude::*;

fn main() {
    println!("hello from purestd");
}

purestd::entry!(main);
```

`entry!` emits the standard C `main` symbol, so the program is started the
ordinary way: by crt0 in a normal build, or by the fullrust toolchain in a
libc-free build. Either caller passes `argc`/`argv`/`envp`, which purestd records
for [`env`](src/env.rs) before your `main` runs.

## libc or not

* **As a normal library (how this repo is tested):** an ordinary `cargo build`.
  libc supplies the C runtime (`_start`, `memcpy`, …); purestd supplies the panic
  handler, the global allocator, and `rust_eh_personality`, and does all of its
  *own* work via raw syscalls. The binary links libc but purestd never calls it.
* **Libc-free / fully static:** build through fullrust, which supplies the entry
  point and the `mem*`/unwind symbols itself, producing a static binary with no
  libc at all. That path lives in the fullrust repo, not here.

## Targets

| Target                      | Tested in CI            |
| --------------------------- | ----------------------- |
| `aarch64-apple-darwin`      | build + run             |
| `x86_64-unknown-linux-gnu`  | build + run             |
| `aarch64-unknown-linux-gnu` | build + run             |

The only architecture/OS-specific code lives in [`src/arch/`](src/arch/): one
file per target with the raw `syscallN` wrappers and the syscall number table.
Everything above it is OS-neutral.

## Layout

```
arch/      raw syscall wrappers + number table (per target)
syscall    arch-neutral, Result-returning wrappers (Errno)
allocator  mmap-backed segregated free-list (#[global_allocator])
panic      the #[panic_handler] + rust_eh_personality
rt         exit/abort + Termination + the entry! glue
io fs env process time sync path ffi error net thread   ← the std surface
hash collections   ← own SipHash-1-3 + HashMap/HashSet
```

`core` and `alloc` are re-exported under `std`-shaped paths (`mem`, `cmp`,
`fmt`, `vec`, `collections`, …), so when purestd is aliased as `std` for a
freestanding target, ordinary `use std::io::Write;` / `std::fs::read(..)` /
`HashMap` code resolves here unchanged.

## The `rt` feature (default on)

Gates the std-provided *policy* symbols — the `#[panic_handler]`, the
`#[global_allocator]` static, and `rust_eh_personality`. Disable it
(`default-features = false`) when a host runtime supplies those instead. The
*mechanisms* (syscalls, the allocator type, the `std` surface) are always
available.

## Building

```sh
cargo run --example stdshow -- some args   # any example, normal build
cargo build --examples
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
`thread_create_running` (no libpthread — see [`docs/macos-threads.md`](docs/macos-threads.md));
on Linux they use `clone` + `futex`. Exercised on every target in CI.

Not yet: `process::Command`, sockets (`net`), buffered readers/writers, TLS,
thread names/parking. `net` is a compiling placeholder.

## License

MIT OR Apache-2.0.
