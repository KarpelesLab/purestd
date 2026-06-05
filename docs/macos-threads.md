# macOS arm64 threads without libc — the working recipe

Validated on Darwin 25 (macOS 26), Apple Silicon. Two dead ends and one path:

## Dead end: `bsdthread_create`
`dyld` loads `libSystem`, whose `libpthread` initializer calls
`bsdthread_register` *before* our `_start`. The kernel allows it once per
process (`kern_support.c`: `if (proc_get_register(p) != 0) return EINVAL`), so we
can never register our own thread-start trampoline. Confirmed: returns EINVAL.

## Dead end: classic `mach_msg_trap` (-31)
Hangs on modern macOS — sends now go through `mach_msg2`.

## Working path: Mach `thread_create_running` via `mach_msg2_trap`
- `task_self_trap` (-28) → task port; `mach_reply_port` (-26) → reply port.
- Build a `mach_msg2` request for `thread_create_running` (MIG id **3412**),
  flavor `ARM_THREAD_STATE64` (6, count 68), setting `pc`, `sp`, `x0` directly.
- Trap is **`mach_msg2_trap` (-47)**, 8 args, header fields packed:
  `bits|size<<32`, `remote|local<<32`, `voucher|id<<32`, `desc|rcvname<<32`,
  `rcvsize|prio<<32`.
- **Critical:** options must include `MACH64_SEND_ANY` (0x800000000) to bypass
  the kernel's mach_msg CFI enforcement (our raw caller isn't the blessed
  libsyscall site). With `MQ_CALL` instead, the send hangs.
- The new thread starts at `pc` with `sp`/`x0` set — no libpthread, no TSD.

## Join + termination
- Sync via `__ulock_wait` (515) / `__ulock_wake` (516) on a 32-bit word
  (`UL_COMPARE_AND_WAIT|ULF_NO_ERRNO` = 0x1000001). `ulock_wake` uses the address
  as a kernel key only (no user deref), so it's safe to free the shared word
  right after observing completion.
- Thread exits via `bsdthread_terminate` (361) `(stackbase, stacksize,
  mach_thread_self(), 0)`, which frees its own stack and terminates cleanly.
