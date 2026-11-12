use std::sync::{Mutex, MutexGuard, PoisonError};

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn wait_until(mut done: impl FnMut() -> bool) {
    while !done() {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
