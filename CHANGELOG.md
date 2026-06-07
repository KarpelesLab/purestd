# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.4](https://github.com/KarpelesLab/purestd/compare/v0.0.3...v0.0.4) - 2026-06-07

### Other

- README — document all eight targets
- explicitly link libc (-lc) to define the crt support symbols
- link wasi-libc (default-linker-libraries) to resolve __wasi_init_tp
- add wasm32 (WASI preview1) backend, reduced surface
- add macOS x86_64 backend (build-only)
- 32-bit timeval for SO_RCVTIMEO/SO_SNDTIMEO
- add i686 + arm (32-bit) Linux backends
- use renameat2 (no legacy renameat on this arch)
- add riscv64-linux backend + qemu-user CI harness

## [0.0.3](https://github.com/KarpelesLab/purestd/compare/v0.0.2...v0.0.3) - 2026-06-06

### Added

- *(collections,os,backtrace)* set ops, retain, MetadataExt, Backtrace stub
- *(io,panic)* stdio locking + Stdin::read_line + catch_unwind (Phase 0/3)
- *(thread)* scoped threads (thread::scope)
- *(tls)* thread_local! + LocalKey; current() real tid (Phase 2)
- *(net,thread,path)* socket options, available_parallelism, path methods
- *(process)* Command/Child/Stdio/Output via fork+execve (Phase 2)
- *(fs,env)* metadata/read_dir/rename/copy + set_var/current_dir (Phase 1)
- *(sync)* futex Mutex + Condvar/Barrier/LazyLock/mpsc (Phase 1)
- *(os,io)* os::fd (RawFd/OwnedFd/AsRawFd) + fuller errno mapping (Phase 0)
- *(io,time)* monotonic Instant, Seek, buffered I/O (Phase 0)

### Fixed

- *(arch)* x86_64 renameat is 264 (RENAME 82 is 2-arg); syscall::rename uses renameat

### Other

- remove ROADMAP — std parity achieved
- ROADMAP — declare std parity achieved
- park / park_timeout / unpark via a futex parker
- futex RwLock; io: vectored I/O; path: Components/ancestors
- add ROADMAP.md (path to std parity)

## [0.0.2](https://github.com/KarpelesLab/purestd/compare/v0.0.1...v0.0.2) - 2026-06-06

### Added

- implement net (TcpStream/TcpListener/UdpSocket + DNS)
