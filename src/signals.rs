use std::sync::atomic::{AtomicBool, Ordering};

const SLOTS: usize = 64;

static PENDING: [AtomicBool; SLOTS] = [const { AtomicBool::new(false) }; SLOTS];
static ANY_PENDING: AtomicBool = AtomicBool::new(false);

extern "C" fn record(signal: libc::c_int) {
    if let Some(slot) = PENDING.get(signal as usize) {
        slot.store(true, Ordering::SeqCst);
        ANY_PENDING.store(true, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Disposition {
    Default,
    Ignore,
    Catch,
}

pub fn set(signal: libc::c_int, disposition: Disposition) {
    let handler = match disposition {
        Disposition::Default => libc::SIG_DFL,
        Disposition::Ignore => libc::SIG_IGN,
        Disposition::Catch => record as extern "C" fn(libc::c_int) as libc::sighandler_t,
    };
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handler;
        action.sa_flags = 0;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(signal, &action, std::ptr::null_mut());
    }
}

pub fn any_pending() -> bool {
    ANY_PENDING.load(Ordering::SeqCst)
}

pub fn take_pending() -> Vec<libc::c_int> {
    if !ANY_PENDING.swap(false, Ordering::SeqCst) {
        return Vec::new();
    }
    PENDING
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.swap(false, Ordering::SeqCst))
        .map(|(signal, _)| signal as libc::c_int)
        .collect()
}

pub fn clear() {
    for slot in &PENDING {
        slot.store(false, Ordering::SeqCst);
    }
    ANY_PENDING.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caught_signals_are_recorded_once() {
        set(libc::SIGUSR2, Disposition::Catch);
        unsafe {
            libc::raise(libc::SIGUSR2);
        }
        assert!(any_pending());
        assert_eq!(take_pending(), [libc::SIGUSR2]);
        assert!(take_pending().is_empty());
        set(libc::SIGUSR2, Disposition::Default);
    }
}
