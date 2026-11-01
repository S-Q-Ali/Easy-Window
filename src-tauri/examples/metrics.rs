//! Task 6.5 overhead harness: what one scheduler tick costs, and what the
//! sampling process costs itself.
//!
//! Run with:
//!
//! ```text
//! cargo run --release --manifest-path src-tauri/Cargo.toml --example metrics
//! cargo run --release --manifest-path src-tauri/Cargo.toml --example metrics -- 600
//! ```
//!
//! The first argument is how many seconds to run; the default is 60. The
//! budget this exists to check is idle CPU under 1 % and RAM under 80 MB.
//!
//! Ticks are driven here rather than by `Scheduler::spawn` because the harness
//! needs each tick's wall time, and the loop thread does not report it. The
//! cadence stays the scheduler's own, so the tick count and the GPU collect
//! count differ by design.

use std::time::{Duration, Instant};

use easy_window_lib::monitor::scheduler::{
    DEFAULT_GPU_INTERVAL, DEFAULT_PROCESS_INTERVAL, Scheduler,
};
use easy_window_lib::monitor::types::GpuTelemetry;

/// A run's tick times, in order, with the first tick kept apart: it verifies
/// every binary it finds, which takes seconds, and must not distort the
/// steady-state figures.
struct Timings {
    first: Option<Duration>,
    steady: Vec<Duration>,
}

impl Timings {
    fn add(&mut self, elapsed: Duration) {
        if self.first.is_none() {
            self.first = Some(elapsed);
        } else {
            self.steady.push(elapsed);
        }
    }

    fn median(&self) -> Duration {
        let mut sorted = self.steady.clone();
        sorted.sort();
        sorted
            .get(sorted.len() / 2)
            .copied()
            .unwrap_or(Duration::ZERO)
    }

    fn mean(&self) -> Duration {
        if self.steady.is_empty() {
            return Duration::ZERO;
        }
        self.steady.iter().copied().sum::<Duration>() / self.steady.len() as u32
    }
}

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(60);

    let mut scheduler = Scheduler::new().expect("scheduler");
    let interval = DEFAULT_PROCESS_INTERVAL;
    let own_pid = std::process::id();

    let mut timings = Timings {
        first: None,
        steady: Vec::new(),
    };
    let mut self_cpu = Vec::new();
    let mut peak_ram = 0u64;
    let mut rows = Spread::default();
    let mut unmeasured = Spread::default();
    let mut payload_bytes = Spread::default();
    let mut gpu = GpuCounts::default();

    println!(
        "sampling {own_pid} for {seconds} s at {DEFAULT_PROCESS_INTERVAL:?} process / {DEFAULT_GPU_INTERVAL:?} GPU"
    );

    for index in 0..seconds.max(1) {
        let start = Instant::now();
        let tick = scheduler.tick();
        let elapsed = start.elapsed();
        timings.add(elapsed);

        // Sized outside the tick above. Task 7 has to send this many bytes per
        // tick, and the figure it needs is the size, not a polluted timing.
        if let Ok(json) = serde_json::to_vec(&tick) {
            payload_bytes.observe(json.len() / 1024);
        }

        if let Some(row) = tick.processes.iter().find(|row| row.process.pid == own_pid) {
            if let Some(cpu) = row.process.cpu_percent {
                self_cpu.push(cpu);
            }
            peak_ram = peak_ram.max(row.process.memory_bytes);
        }

        rows.observe(tick.processes.len());
        unmeasured.observe(tick.summary.unmeasured_processes);
        gpu.observe(&tick.gpu);

        if (index + 1) % 30 == 0 {
            println!(
                "  tick {:>4}: {:>7.1?} | {} rows | self cpu {} % | gpu {:?}",
                tick.sequence,
                elapsed,
                tick.processes.len(),
                self_cpu
                    .last()
                    .copied()
                    .map_or("-".to_string(), |c| format!("{c:.3}")),
                tick.gpu,
            );
        }

        std::thread::sleep(interval.saturating_sub(elapsed));
    }

    let mut cpu_sorted = self_cpu.clone();
    cpu_sorted.sort_by(f32::total_cmp);
    let cpu_mean = if self_cpu.is_empty() {
        0.0
    } else {
        self_cpu.iter().sum::<f32>() / self_cpu.len() as f32
    };
    let cpu_peak = cpu_sorted.last().copied().unwrap_or_default();
    let cpu_median = cpu_sorted
        .get(cpu_sorted.len() / 2)
        .copied()
        .unwrap_or_default();
    let over_budget = self_cpu.iter().filter(|cpu| **cpu >= 1.0).count();

    println!("---- steady state over {} ticks ----", timings.steady.len());
    println!(
        "first tick {:?} (every signature verified once), then min {:?} / median {:?} / mean {:?} / max {:?}",
        timings.first.unwrap_or(Duration::ZERO),
        timings
            .steady
            .iter()
            .min()
            .copied()
            .unwrap_or(Duration::ZERO),
        timings.median(),
        timings.mean(),
        timings
            .steady
            .iter()
            .max()
            .copied()
            .unwrap_or(Duration::ZERO),
    );
    println!(
        "own cpu mean {cpu_mean:.3} %, median {cpu_median:.3} %, peak {cpu_peak:.3} % (budget: < 1 %), {over_budget} of {} ticks at or over it",
        self_cpu.len(),
    );
    println!("peak ram {} MB (budget: < 80 MB)", peak_ram / 1_048_576);
    println!(
        "mean tick {:?} of a {interval:?} interval = {:.2} % duty",
        timings.mean(),
        timings.mean().as_secs_f32() / interval.as_secs_f32() * 100.0
    );
    println!(
        "rows {rows} | no CPU figure {unmeasured} | tick json {payload_bytes} KB | gpu: {}",
        gpu.summary()
    );
}

/// What the run saw from the GPU clock: how often it collected, and the
/// largest engine counts and worst parse failure it reported.
#[derive(Default)]
struct GpuCounts {
    sampling_ticks: usize,
    other_ticks: usize,
    engines_sampled: usize,
    engines_unsampled: usize,
    unparsed: usize,
    last_other: String,
}

impl GpuCounts {
    fn observe(&mut self, state: &GpuTelemetry) {
        match state {
            GpuTelemetry::Sampling {
                engines_sampled,
                engines_unsampled,
                unparsed,
            } => {
                self.sampling_ticks += 1;
                self.engines_sampled = self.engines_sampled.max(*engines_sampled);
                self.engines_unsampled = self.engines_unsampled.max(*engines_unsampled);
                self.unparsed = self.unparsed.max(*unparsed);
            }
            other => {
                self.other_ticks += 1;
                self.last_other = format!("{other:?}");
            }
        }
    }

    fn summary(&self) -> String {
        format!(
            "{} collected, {} not (peak {} engines sampled, {} unsampled, {} unparsed){}",
            self.sampling_ticks,
            self.other_ticks,
            self.engines_sampled,
            self.engines_unsampled,
            self.unparsed,
            if self.other_ticks == 0 {
                String::new()
            } else {
                format!(", last {:?}", self.last_other)
            },
        )
    }
}

/// The low, average and high of a count over a run, because a peak on its own
/// hides how much the process list moves between ticks.
#[derive(Default)]
struct Spread {
    samples: usize,
    total: usize,
    min: Option<usize>,
    max: usize,
}

impl Spread {
    fn observe(&mut self, value: usize) {
        self.samples += 1;
        self.total += value;
        self.min = Some(self.min.map_or(value, |min| min.min(value)));
        self.max = self.max.max(value);
    }

    fn mean(&self) -> usize {
        self.total.checked_div(self.samples).unwrap_or_default()
    }
}

impl std::fmt::Display for Spread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "min {} / mean {} / max {}",
            self.min.unwrap_or_default(),
            self.mean(),
            self.max
        )
    }
}
