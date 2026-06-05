//! Test-only scaffolding — **not part of the purestd library.**
//!
//! purestd is only `std`. The process entry point and the toolchain-expected
//! symbols (`memcpy`/`memset`/…, the unwind stubs, aarch64 `getauxval`) are not
//! purestd's concern: in a real build they come from crt0 / compiler_builtins /
//! the unwinder, which here is **fullrust's** job.
//!
//! So the examples can still build and run standalone (for the dev loop and CI),
//! each one pulls in this minimal stand-in via:
//!
//! ```ignore
//! #[path = "common/rt.rs"]
//! mod rt;
//! ```
//!
//! Anything fullrust provides for real, this provides just enough of for a test
//! binary to link and run.

use core::ffi::{c_char, c_int};

extern "C" {
    /// purestd's `lang_start`-equivalent glue (the part that *is* std).
    fn __purestd_start(argc: usize, argv: *const *const u8, envp: *const *const u8) -> !;
}

// ---- process entry (crt0's job) ----

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start(
    argc: usize,
    argv: *const *const u8,
    envp: *const *const u8,
) -> ! {
    __purestd_start(argc, argv, envp)
}

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
    let envp = argv.add(argc + 1);
    __purestd_start(argc, argv, envp)
}

// ---- mem* intrinsics (compiler_builtins / libc's job) ----

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *dest.add(i) = *src.add(i);
        i += 1;
    }
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if (dest as usize) < (src as usize) {
        let mut i = 0;
        while i < n {
            *dest.add(i) = *src.add(i);
            i += 1;
        }
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            *dest.add(i) = *src.add(i);
        }
    }
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dest: *mut u8, c: c_int, n: usize) -> *mut u8 {
    let byte = c as u8;
    let mut i = 0;
    while i < n {
        *dest.add(i) = byte;
        i += 1;
    }
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bzero(dest: *mut u8, n: usize) {
    let _ = memset(dest, 0, n);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> c_int {
    let mut i = 0;
    while i < n {
        let (x, y) = (*a.add(i), *b.add(i));
        if x != y {
            return x as c_int - y as c_int;
        }
        i += 1;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcmp(a: *const u8, b: *const u8, n: usize) -> c_int {
    memcmp(a, b, n)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let mut n = 0;
    while *s.add(n) != 0 {
        n += 1;
    }
    n
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getauxval(_type_: core::ffi::c_ulong) -> core::ffi::c_ulong {
    0
}

// ---- unwind abort-stubs (never executed under panic = "abort") ----

#[unsafe(no_mangle)]
extern "C" fn rust_eh_personality() {}

#[unsafe(no_mangle)]
extern "C" fn _Unwind_Resume() -> ! {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("udf #0", options(noreturn))
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("ud2", options(noreturn))
    }
}
