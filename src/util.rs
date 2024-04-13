use std::time::{Duration, Instant};

pub(crate) fn measure_exec<F: FnOnce()>(name: &str, f: F, threshold: Duration) {
    let start = Instant::now();
    f();
    let duration = start.elapsed();
    if duration > threshold {
        log::warn!("Calling {name} took {duration:?}");
    }
}
