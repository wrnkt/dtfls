//! Ctrl-C handling. Outside a critical section, Ctrl-C prints "Interrupted."
//! and exits 130 straight away. Inside one (the backup branch switch), it is
//! deferred until the section ends so the original branch is always restored.

use std::sync::atomic::{AtomicBool, Ordering};

static CRITICAL: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicBool = AtomicBool::new(false);

pub fn install() {
    let _ = ctrlc::set_handler(|| {
        if CRITICAL.load(Ordering::SeqCst) {
            PENDING.store(true, Ordering::SeqCst);
        } else {
            exit_interrupted();
        }
    });
}

fn exit_interrupted() -> ! {
    println!();
    crate::ui::warn("Interrupted.");
    std::process::exit(130);
}

/// Defers Ctrl-C while alive; exits on drop if one arrived meanwhile.
pub struct Critical(());

impl Critical {
    pub fn enter() -> Self {
        CRITICAL.store(true, Ordering::SeqCst);
        Critical(())
    }
}

impl Drop for Critical {
    fn drop(&mut self) {
        CRITICAL.store(false, Ordering::SeqCst);
        if PENDING.swap(false, Ordering::SeqCst) {
            exit_interrupted();
        }
    }
}
