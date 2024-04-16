use std::time::{Duration, Instant};

pub(crate) fn measure_exec<F: FnOnce()>(name: &str, f: F, threshold: Duration) {
    let start = Instant::now();
    f();
    let end = Instant::now();
    let duration = end.checked_duration_since(start);
    if let Some(duration) = duration {
        if duration > threshold {
            log::warn!("Calling {name} took {duration:?}");
        }
    }
}
