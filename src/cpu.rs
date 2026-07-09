use esp_idf_svc::sys::{uxTaskGetNumberOfTasks, uxTaskGetSystemState, TaskStatus_t};
use std::collections::HashMap;
use std::ffi::CStr;
use std::time::Duration;

struct Snapshot {
    /// Total run time (esp_timer microseconds) at the moment of the snapshot.
    total: u64,
    /// (task handle, name, cumulative run-time counter in microseconds).
    tasks: Vec<(usize, String, u64)>,
}

fn snapshot() -> Snapshot {
    // +8 slack in case a task is created between the count and the query
    let cap = unsafe { uxTaskGetNumberOfTasks() } as usize + 8;
    let mut arr: Vec<TaskStatus_t> = Vec::with_capacity(cap);
    let mut total: u32 = 0;
    let tasks = unsafe {
        let filled = uxTaskGetSystemState(arr.as_mut_ptr(), cap as u32, &mut total) as usize;
        arr.set_len(filled);
        arr.iter()
            .map(|t| {
                let name = CStr::from_ptr(t.pcTaskName).to_string_lossy().into_owned();
                (t.xHandle as usize, name, t.ulRunTimeCounter as u64)
            })
            .collect()
    };
    Snapshot {
        total: total as u64,
        tasks,
    }
}

fn report(a: &Snapshot, b: &Snapshot) {
    let dtotal = b.total.wrapping_sub(a.total);
    if dtotal == 0 {
        return;
    }
    let prev: HashMap<usize, u64> = a.tasks.iter().map(|(h, _, c)| (*h, *c)).collect();

    // Percentages are of one core's time: a task pinned to a core tops out near
    // 100%, and all tasks together sum to ~200% across the two cores.
    let mut rows: Vec<(f64, &str)> = Vec::new();
    for (h, name, c) in &b.tasks {
        let Some(pc) = prev.get(h) else { continue };
        let pct = c.wrapping_sub(*pc) as f64 * 100.0 / dtotal as f64;
        rows.push((pct, name.as_str()));
    }
    rows.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());

    // the two IDLE tasks are pinned one per core, so their share is that core's
    // free budget
    let idle = |n: &str| {
        rows.iter()
            .find(|(_, name)| *name == n)
            .map_or(f64::NAN, |(p, _)| *p)
    };
    log::info!(
        "CPU free budget: core0 {:.0}%  core1 {:.0}%  (100% = one idle core)",
        idle("IDLE0"),
        idle("IDLE1"),
    );
    for (pct, name) in rows.iter().take(10) {
        if *pct >= 1.0 && !name.starts_with("IDLE") {
            log::info!("  {name:>12}: {pct:5.1}%");
        }
    }
}

fn monitor(window: Duration) -> ! {
    loop {
        let a = snapshot();
        std::thread::sleep(window);
        let b = snapshot();
        report(&a, &b);
    }
}

pub fn spawn() {
    std::thread::Builder::new()
        .name("cpumon".into())
        .stack_size(4096)
        .spawn(|| monitor(Duration::from_secs(10)))
        .unwrap();
}
