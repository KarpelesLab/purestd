//! The process entry point — crt0's job, not std's.
//!
//! `_start` is where the kernel/loader hands over control. It decodes the
//! argument/environment vectors (registers on macOS, the initial stack on Linux)
//! and tail-calls `purestd`'s runtime start symbol `__purestd_start`, which runs
//! the user `main` and exits.

extern "C" {
    /// Defined by `purestd` (its `std::rt`-equivalent glue).
    fn __purestd_start(argc: usize, argv: *const *const u8, envp: *const *const u8) -> !;
}

// --- macOS/arm64: the loader calls the entry like `main(argc, argv, envp)` with
// the values already in x0..x2 (Mach-O LC_MAIN convention). ---
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(
    argc: usize,
    argv: *const *const u8,
    envp: *const *const u8,
) -> ! {
    __purestd_start(argc, argv, envp)
}

// --- Linux: the kernel passes them on the initial stack (`sp -> argc, argv..,
// envp..`). A naked `_start` captures `sp` untouched and tail-calls `rust_start`. ---
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "xor rbp, rbp",
        "mov rdi, rsp",
        "and rsp, -16",
        "call {s}",
        s = sym rust_start,
    )
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "mov x0, sp",
        "and x1, x0, #-16",
        "mov sp, x1",
        "b {s}",
        s = sym rust_start,
    )
}

#[cfg(target_os = "linux")]
unsafe extern "C" fn rust_start(stack: *const usize) -> ! {
    let argc = *stack;
    let argv = stack.add(1) as *const *const u8;
    // argv[argc] is NULL; envp follows it.
    let envp = argv.add(argc + 1);
    __purestd_start(argc, argv, envp)
}
