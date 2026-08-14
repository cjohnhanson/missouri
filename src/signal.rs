use std::io;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static FORCE_EXIT: AtomicBool = AtomicBool::new(false);

/// Fixed-size slot array that tracks the active child PIDs. It uses no
/// locks and is safe to touch from a signal handler. 64 slots are more
/// than any real test run needs.
const MAX_CHILDREN: usize = 64;
static CHILD_PIDS: [AtomicU32; MAX_CHILDREN] = {
    // const initializer — can't use a loop, so use a macro
    macro_rules! zeros {
        ($($i:expr),*) => { [$(AtomicU32::new({ let _ = $i; 0 })),*] }
    }
    zeros!(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63
    )
};

pub fn is_interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

pub fn set_interrupted() {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

pub fn is_force_exit() -> bool {
    FORCE_EXIT.load(Ordering::SeqCst)
}

pub fn set_force_exit() {
    FORCE_EXIT.store(true, Ordering::SeqCst);
}

/// Register a child PID in the first available slot. Returns the slot index.
pub fn register_child(pid: u32) -> usize {
    for (i, slot) in CHILD_PIDS.iter().enumerate() {
        if slot
            .compare_exchange(0, pid, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return i;
        }
    }
    // Every slot is full. 64 slots make this unlikely. Do not panic.
    0
}

/// Clear a child PID slot.
pub fn clear_child(slot: usize) {
    if slot < MAX_CHILDREN {
        CHILD_PIDS[slot].store(0, Ordering::SeqCst);
    }
}

/// Kill all registered child PIDs with the given signal.
pub fn kill_all_children(sig: i32) {
    for slot in &CHILD_PIDS {
        let pid = slot.load(Ordering::SeqCst);
        if pid != 0 {
            unsafe { libc::kill(pid as i32, sig) };
        }
    }
}

/// Spawn a command and register its PID for signal handling. Wait for the
/// output, then clear the PID registration. This replaces `.output()`.
pub fn run_tracked(cmd: &mut Command) -> io::Result<Output> {
    let child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let slot = register_child(child.id());
    let output = child.wait_with_output();
    clear_child(slot);
    output
}
