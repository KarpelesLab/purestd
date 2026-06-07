//! OS thread primitives: create a thread on a fresh stack, a futex for
//! join/parking, sleep, and yield. The upper `thread` module builds the
//! `std::thread` API on these.
//!
//! * **macOS/arm64** uses the Mach `thread_create_running` path (see
//!   `docs/macos-threads.md`): `bsdthread_create` is unavailable to us because
//!   libpthread already owns `bsdthread_register`. Validated and runs natively.
//! * **Linux** uses the `clone` syscall via a small asm wrapper (musl-style),
//!   plus the `futex` syscall. Compile-verified; not yet runtime-tested here.

use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;

/// Entry signature for a freshly created thread. Receives the raw payload
/// pointer and never returns (it terminates the thread).
pub type ThreadEntry = extern "C" fn(*mut u8) -> !;

// ===========================================================================
// macOS / arm64
// ===========================================================================
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod imp {
    use super::*;
    use crate::sync::OnceLock;
    use core::arch::asm;

    // Mach traps (negative trap numbers in x16).
    const MACH_REPLY_PORT: isize = -26;
    const TASK_SELF_TRAP: isize = -28;
    const THREAD_SELF_TRAP: isize = -27;
    const THREAD_SWITCH: isize = -61;
    const MACH_MSG2_TRAP: isize = -47;
    // BSD syscalls (class | number).
    const SYS_BSDTHREAD_TERMINATE: isize = 0x2000000 | 361;
    const SYS_ULOCK_WAIT: isize = 0x2000000 | 515;
    const SYS_ULOCK_WAKE: isize = 0x2000000 | 516;
    // UL_COMPARE_AND_WAIT | ULF_NO_ERRNO
    const UL_OP: usize = 1 | 0x1000000;
    // mach_msg2 options: SEND | RCV | SEND_ANY (bypass CFI for our raw caller).
    const MSG_OPT: usize = 1 | 2 | 0x800000000;

    #[inline]
    unsafe fn svc(
        n: isize,
        a0: usize,
        a1: usize,
        a2: usize,
        a3: usize,
        a4: usize,
        a5: usize,
        a6: usize,
        a7: usize,
    ) -> isize {
        let mut r: isize;
        asm!(
            "svc #0x80",
            inout("x16") n => _,
            lateout("x17") _,
            inout("x0") a0 => r,
            in("x1") a1, in("x2") a2, in("x3") a3,
            in("x4") a4, in("x5") a5, in("x6") a6, in("x7") a7,
            options(nostack),
        );
        r
    }
    macro_rules! svcn {
        ($n:expr $(, $a:expr)*) => {{
            let args = [$($a as usize),*];
            let mut it = args.iter().copied();
            svc($n,
                it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0),
                it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0),
                it.next().unwrap_or(0), it.next().unwrap_or(0))
        }};
    }

    #[repr(C)]
    struct MsgHdr {
        bits: u32,
        size: u32,
        remote: u32,
        local: u32,
        voucher: u32,
        id: i32,
    }
    #[repr(C)]
    struct CreateReq {
        h: MsgHdr,
        ndr: [u8; 8],
        flavor: i32,
        count: u32,
        state: [u64; 34], // ARM_THREAD_STATE64 (68 u32)
    }

    fn ports() -> (u32, u32) {
        static PORTS: OnceLock<(u32, u32)> = OnceLock::new();
        *PORTS.get_or_init(|| unsafe {
            let task = svcn!(TASK_SELF_TRAP) as u32;
            let reply = svcn!(MACH_REPLY_PORT) as u32;
            (task, reply)
        })
    }

    // Serializes thread creation (the reply port is shared).
    static CREATE_LOCK: AtomicU32 = AtomicU32::new(0);
    fn lock() {
        while CREATE_LOCK
            .compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }
    fn unlock() {
        CREATE_LOCK.store(0, Ordering::Release);
    }

    pub unsafe fn spawn_os(entry: ThreadEntry, arg: *mut u8, stack_top: usize) -> Result<(), ()> {
        let (task, reply) = ports();
        let mut req: CreateReq = core::mem::zeroed();
        req.h.bits = 0x1513; // remote COPY_SEND(19) | local MAKE_SEND_ONCE(21)<<8
        req.h.remote = task;
        req.h.local = reply;
        req.h.id = 3412; // thread_create_running
        req.flavor = 6; // ARM_THREAD_STATE64
        req.count = 68;
        req.state[0] = arg as u64; // x0
        req.state[31] = stack_top as u64; // sp
        req.state[32] = entry as usize as u64; // pc
        let send_size: u32 = (core::mem::size_of::<MsgHdr>() + 8 + 4 + 4 + 68 * 4) as u32;
        req.h.size = send_size;

        let bits_size = req.h.bits as usize | ((send_size as usize) << 32);
        let rem_loc = task as usize | ((reply as usize) << 32);
        let vouch_id = (req.h.id as u32 as usize) << 32;
        let desc_rcv = (reply as usize) << 32;
        let rcvsz_prio = core::mem::size_of::<CreateReq>();

        lock();
        let kr = svc(
            MACH_MSG2_TRAP,
            &mut req as *mut _ as usize,
            MSG_OPT,
            bits_size,
            rem_loc,
            vouch_id,
            desc_rcv,
            rcvsz_prio,
            0,
        );
        unlock();
        if kr == 0 {
            Ok(())
        } else {
            Err(())
        }
    }

    pub unsafe fn thread_exit(stack_base: usize, stack_size: usize) -> ! {
        let self_port = svcn!(THREAD_SELF_TRAP) as usize;
        // bsdthread_terminate(stackbase, stacksize, port, sem) frees the stack
        // and terminates this thread.
        svcn!(SYS_BSDTHREAD_TERMINATE, stack_base, stack_size, self_port, 0usize);
        loop {
            core::hint::spin_loop();
        }
    }

    pub fn futex_wait(addr: &AtomicU32, expected: u32) {
        while addr.load(Ordering::Acquire) == expected {
            unsafe {
                svcn!(SYS_ULOCK_WAIT, UL_OP, addr as *const _ as usize, expected as usize, 0usize);
            }
        }
    }

    /// Wait until `*addr != expected` or `dur` elapses (single-shot; a spurious
    /// early return is allowed). The `__ulock` timeout is in microseconds.
    pub fn futex_wait_timeout(addr: &AtomicU32, expected: u32, dur: Duration) {
        if addr.load(Ordering::Acquire) != expected {
            return;
        }
        let us = core::cmp::min(dur.as_micros(), u32::MAX as u128) as usize;
        unsafe {
            svcn!(SYS_ULOCK_WAIT, UL_OP, addr as *const _ as usize, expected as usize, us);
        }
    }

    pub fn futex_wake(addr: &AtomicU32) {
        unsafe {
            svcn!(SYS_ULOCK_WAKE, UL_OP, addr as *const _ as usize, 0usize);
        }
    }

    pub fn futex_wake_all(addr: &AtomicU32) {
        // ULF_WAKE_ALL = 0x100
        unsafe {
            svcn!(SYS_ULOCK_WAKE, UL_OP | 0x100, addr as *const _ as usize, 0usize);
        }
    }

    pub fn sleep(dur: Duration) {
        // ulock_wait with a timeout on a word that never changes acts as a
        // sleep. The timeout is microseconds (u32); loop for long durations.
        let mut us = dur.as_micros();
        let dummy = AtomicU32::new(0);
        while us > 0 {
            let chunk = core::cmp::min(us, u32::MAX as u128) as usize;
            unsafe {
                svcn!(SYS_ULOCK_WAIT, UL_OP, &dummy as *const _ as usize, 0usize, chunk);
            }
            us -= chunk as u128;
        }
    }

    pub fn yield_now() {
        // thread_switch(THREAD_NULL, SWITCH_OPTION_NONE, 0)
        unsafe {
            svcn!(THREAD_SWITCH, 0usize, 0usize, 0usize);
        }
    }
}

// ===========================================================================
// Linux (x86_64 / aarch64)
// ===========================================================================
#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::arch::{self, nr};

    // Child-stack-launching clone wrapper (musl-style). Defined in asm so the
    // child begins on the new stack and calls `entry(arg)` then `exit`.
    extern "C" {
        fn __purestd_clone(
            entry: ThreadEntry,
            stack_top: *mut u8,
            flags: usize,
            arg: *mut u8,
        ) -> isize;
    }

    #[cfg(target_arch = "x86_64")]
    core::arch::global_asm!(
        ".text",
        ".globl __purestd_clone",
        "__purestd_clone:",          // rdi=entry, rsi=stack_top, rdx=flags, rcx=arg
        "  and rsi, -16",
        "  sub rsi, 16",
        "  mov [rsi], rdi",          // save entry on child stack
        "  mov [rsi+8], rcx",        // save arg
        "  mov rdi, rdx",            // clone arg1 = flags
        "  xor edx, edx",            // parent_tid = 0
        "  xor r10d, r10d",          // child_tid = 0
        "  xor r8d, r8d",            // tls = 0
        "  mov eax, 56",             // SYS_clone
        "  syscall",
        "  test rax, rax",
        "  jnz 1f",                  // parent: return tid
        "  xor ebp, ebp",
        "  pop rax",                 // child: rax = entry
        "  pop rdi",                 // rdi = arg
        "  call rax",                // entry(arg); never returns
        "  mov eax, 60",             // SYS_exit (fallback)
        "  xor edi, edi",
        "  syscall",
        "1:",
        "  ret",
    );

    #[cfg(target_arch = "aarch64")]
    core::arch::global_asm!(
        ".text",
        ".globl __purestd_clone",
        "__purestd_clone:",          // x0=entry, x1=stack_top, x2=flags, x3=arg
        "  and x1, x1, #-16",
        "  stp x0, x3, [x1, #-16]!", // save entry,arg on child stack
        "  mov x0, x2",              // clone flags
        "  mov x2, #0",              // parent_tid
        "  mov x3, #0",              // tls
        "  mov x4, #0",              // child_tid
        "  mov x8, #220",            // SYS_clone
        "  svc #0",
        "  cbz x0, 2f",              // child?
        "  ret",                     // parent: return tid
        "2:",
        "  ldp x1, x0, [sp], #16",   // x1=entry, x0=arg
        "  blr x1",                  // entry(arg); never returns
        "  mov x8, #93",             // SYS_exit (fallback)
        "  svc #0",
    );

    #[cfg(target_arch = "riscv64")]
    core::arch::global_asm!(
        ".text",
        ".globl __purestd_clone",
        "__purestd_clone:",          // a0=entry, a1=stack_top, a2=flags, a3=arg
        "  andi a1, a1, -16",        // align child stack
        "  addi a1, a1, -16",        // reserve a slot for entry+arg
        "  sd   a0, 0(a1)",          // save entry on child stack
        "  sd   a3, 8(a1)",          // save arg
        "  mv   a0, a2",             // clone arg0 = flags (new_sp already in a1)
        "  li   a2, 0",              // parent_tid
        "  li   a3, 0",              // tls
        "  li   a4, 0",              // child_tid
        "  li   a7, 220",            // SYS_clone
        "  ecall",
        "  bnez a0, 1f",             // parent: a0 = child tid
        "  ld   a1, 0(sp)",          // child: a1 = entry
        "  ld   a0, 8(sp)",          // a0 = arg
        "  jalr a1",                 // entry(arg); never returns
        "  li   a7, 93",             // SYS_exit (fallback)
        "  li   a0, 0",
        "  ecall",
        "1:",
        "  ret",
    );

    // Thread creation flags (shared VM, fds, signals, same thread group).
    const CLONE_VM: usize = 0x00000100;
    const CLONE_FS: usize = 0x00000200;
    const CLONE_FILES: usize = 0x00000400;
    const CLONE_SIGHAND: usize = 0x00000800;
    const CLONE_THREAD: usize = 0x00010000;
    const CLONE_SYSVSEM: usize = 0x00040000;

    pub unsafe fn spawn_os(entry: ThreadEntry, arg: *mut u8, stack_top: usize) -> Result<(), ()> {
        let flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD | CLONE_SYSVSEM;
        let r = __purestd_clone(entry, stack_top as *mut u8, flags, arg);
        if r > 0 {
            Ok(())
        } else {
            Err(())
        }
    }

    pub unsafe fn thread_exit(_stack_base: usize, _stack_size: usize) -> ! {
        // A thread can't unmap the stack it's running on; the page is reclaimed
        // at process exit. (A CLONE_CHILD_CLEARTID-based reaper can free it
        // later.) Just exit this thread.
        arch::syscall1(nr::EXIT, 0);
        loop {
            core::hint::spin_loop();
        }
    }

    // FUTEX_WAIT_PRIVATE / FUTEX_WAKE_PRIVATE
    const FUTEX_WAIT: usize = 0 | 128;
    const FUTEX_WAKE: usize = 1 | 128;

    pub fn futex_wait(addr: &AtomicU32, expected: u32) {
        while addr.load(Ordering::Acquire) == expected {
            unsafe {
                arch::syscall4(
                    nr::FUTEX,
                    addr as *const _ as usize,
                    FUTEX_WAIT,
                    expected as usize,
                    0, // NULL timeout
                );
            }
        }
    }

    /// Wait until `*addr != expected` or `dur` elapses (single-shot). The
    /// `FUTEX_WAIT` timeout is a *relative* `timespec`.
    pub fn futex_wait_timeout(addr: &AtomicU32, expected: u32, dur: Duration) {
        if addr.load(Ordering::Acquire) != expected {
            return;
        }
        let ts = [dur.as_secs() as i64, dur.subsec_nanos() as i64];
        unsafe {
            arch::syscall4(
                nr::FUTEX,
                addr as *const _ as usize,
                FUTEX_WAIT,
                expected as usize,
                ts.as_ptr() as usize,
            );
        }
    }

    pub fn futex_wake(addr: &AtomicU32) {
        unsafe {
            arch::syscall3(nr::FUTEX, addr as *const _ as usize, FUTEX_WAKE, 1);
        }
    }

    pub fn futex_wake_all(addr: &AtomicU32) {
        unsafe {
            arch::syscall3(nr::FUTEX, addr as *const _ as usize, FUTEX_WAKE, i32::MAX as usize);
        }
    }

    pub fn sleep(dur: Duration) {
        // struct timespec { i64 tv_sec; i64 tv_nsec; }
        let ts = [dur.as_secs() as i64, dur.subsec_nanos() as i64];
        unsafe {
            arch::syscall2(nr::NANOSLEEP, ts.as_ptr() as usize, 0);
        }
    }

    pub fn yield_now() {
        unsafe {
            arch::syscall0(nr::SCHED_YIELD);
        }
    }
}

pub use imp::{
    futex_wait, futex_wait_timeout, futex_wake, futex_wake_all, sleep, spawn_os, thread_exit,
    yield_now,
};
