//! Symbols the toolchain expects from the environment when no libc is linked.
//!
//! Without a libc, the compiler's lowering of struct copies / slice fills and
//! the precompiled `alloc` still reference `memcpy`/`memset`/`memmove`/`memcmp`/
//! `bcmp`, `strlen`, and the unwind symbols `rust_eh_personality`/
//! `_Unwind_Resume`. We provide them all here so the binary links with zero
//! external dependencies. These are gated behind the `rt` feature — turn it off
//! if some other crate in the final binary supplies them.
//!
//! The `mem*` loops are deliberately simple; LLVM's loop-idiom pass will not
//! rewrite such a loop into a call to the function it lives in, so naming them
//! `memcpy`/`memset` is safe from self-recursion.

use core::ffi::{c_char, c_int};

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

/// BSD `bzero` — some Darwin codegen paths reference it instead of `memset`.
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

/// Length of a NUL-terminated C string. Used by `core::ffi::CStr::from_ptr`,
/// which we rely on to read `argv`/`envp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let mut n = 0;
    while *s.add(n) != 0 {
        n += 1;
    }
    n
}

/// On aarch64, `compiler_builtins` calls `getauxval` to detect LSE atomics at
/// runtime. With no libc we supply it; returning 0 (no `HWCAP` bits) selects the
/// always-correct LL/SC atomic fallback. A later version can return the real
/// auxv value captured at `_start`.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getauxval(_type_: core::ffi::c_ulong) -> core::ffi::c_ulong {
    0
}

// ---- unwind stubs (never executed under panic = "abort") ----

#[unsafe(no_mangle)]
extern "C" fn rust_eh_personality() {}

#[unsafe(no_mangle)]
extern "C" fn _Unwind_Resume() -> ! {
    // Reaching here means unwinding is happening, which must not occur under
    // panic = "abort". Treat as fatal.
    crate::syscall::exit_group(134)
}
