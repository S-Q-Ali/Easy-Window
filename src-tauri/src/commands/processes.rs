//! The contract between the tick loop and the window: how the UI learns what
//! the loop is doing.
//!
//! One scheduler runs for the whole app (T1). The UI asks it once on mount and
//! is then pushed every tick, so a table can be populated without waiting a
//! second for the first event and without a polling timer of its own.
//!
//! The loop is owned here rather than by the command that reads it, and the
//! `Mutex` is what makes it shareable: `RunningScheduler` holds the control
//! channel's `Sender`, which is `Send` but not `Sync`, so the managed state
//! cannot hold it directly. Dropping this state drops the sender, and the loop
//! stops — which is what app exit does.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter as _, State};

use crate::monitor::{
    scheduler::{RunningScheduler, Scheduler},
    types::Tick,
};

/// The event every tick is pushed on, so the front end names it once.
pub const TICK_EVENT: &str = "metrics://tick";

/// What the sampling loop can tell the UI right now.
///
/// `running` with `tick: null` is a real state, not an error: the loop is alive
/// and its first tick has not been published yet. A cold first tick verifies
/// every binary on the machine and takes seconds.
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SamplingState {
    /// The newest tick, or `None` while the first one is still being measured.
    ///
    /// A tick is a display figure, not an authority to act on. It is up to a
    /// tick-interval old, and within that a PID can exit and be recycled onto a
    /// different process, so `selectable` in a pushed tick says only that the
    /// sampler judged that *snapshot* closable. Task 9 re-resolves identity and
    /// re-runs the denylist at action time.
    Running {
        tick: Option<Tick>,
    },
    Failed {
        reason: String,
    },
}

enum Loop {
    Running(RunningScheduler),
    Failed { reason: String },
}

/// The app's one tick loop, in managed state.
pub struct Metrics {
    loop_: Mutex<Loop>,
}

impl Metrics {
    /// Starts the loop and hands every tick it publishes to the window.
    ///
    /// A loop that cannot start is reported, not fatal: the app still opens,
    /// the settings and audit pages still work, and the process table is told
    /// why it has no numbers.
    pub fn start(app: AppHandle) -> Self {
        let loop_ = match Scheduler::new() {
            Ok(scheduler) => Loop::Running(scheduler.spawn(move |tick| {
                // A window that closed between ticks is not a sampling failure,
                // so the result is dropped. The cost of dropping it is that a
                // persistent failure shows up as a frozen table while
                // `latest_tick` still says running — nothing reports which of the
                // two it is, and that is a diagnostics gap, not a handled case.
                let _ = app.emit(TICK_EVENT, tick);
            })),
            Err(error) => Loop::Failed {
                reason: error.to_string(),
            },
        };

        Metrics::new(loop_)
    }

    fn new(loop_: Loop) -> Self {
        Metrics {
            loop_: Mutex::new(loop_),
        }
    }

    /// The newest tick the loop published, or the reason there is no loop.
    pub fn state(&self) -> SamplingState {
        match &*self.loop_.lock().expect("metrics state lock") {
            Loop::Running(running) => SamplingState::Running {
                tick: running.latest(),
            },
            Loop::Failed { reason } => SamplingState::Failed {
                reason: reason.clone(),
            },
        }
    }
}

/// The first frame for the process table: what the loop has right now.
#[tauri::command]
pub fn latest_tick(metrics: State<'_, Metrics>) -> SamplingState {
    metrics.state()
}

#[cfg(test)]
mod tests {
    use super::{Loop, Metrics, SamplingState};
    use crate::monitor::{
        scheduler::Scheduler,
        types::{GpuTelemetry, SystemSummary, Tick},
    };
    use std::time::{Duration, Instant};

    fn tick(sequence: u64) -> Tick {
        Tick {
            sequence,
            processes: Vec::new(),
            summary: SystemSummary {
                cpu_percent: Some(10.0),
                disk_read_bytes_per_sec: None,
                disk_write_bytes_per_sec: None,
                gpu_percent: None,
                process_count: 0,
                unmeasured_processes: 0,
            },
            gpu: GpuTelemetry::Warming,
            gpu_sequence: None,
        }
    }

    #[test]
    fn a_running_loop_with_no_tick_says_running_and_null() {
        let json = serde_json::to_value(SamplingState::Running { tick: None }).unwrap();

        assert_eq!(
            json,
            serde_json::json!({ "state": "running", "tick": null })
        );
    }

    #[test]
    fn a_running_loop_with_a_tick_sends_the_whole_tick() {
        let json = serde_json::to_value(SamplingState::Running {
            tick: Some(tick(3)),
        })
        .unwrap();

        assert_eq!(json["state"], "running");
        assert_eq!(json["tick"]["sequence"], 3);
        assert_eq!(json["tick"]["summary"]["cpuPercent"], 10.0);
        assert_eq!(json["tick"]["gpu"]["kind"], "warming");
    }

    #[test]
    fn a_loop_that_could_not_start_says_why() {
        let metrics = Metrics::new(Loop::Failed {
            reason: "the machine could not be sampled: no memory".to_string(),
        });

        let json = serde_json::to_value(metrics.state()).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "state": "failed",
                "reason": "the machine could not be sampled: no memory"
            })
        );
    }

    #[test]
    fn a_running_loop_hands_the_state_reader_its_newest_tick() {
        // The real loop, because the whole point is that the reader sees what
        // the loop published rather than a copy taken at command time.
        let metrics = Metrics::new(Loop::Running(
            Scheduler::with_intervals(Duration::from_millis(200), Duration::from_millis(1_400))
                .expect("scheduler")
                .spawn(|_| {}),
        ));

        assert!(
            matches!(metrics.state(), SamplingState::Running { .. }),
            "a live loop is never reported as failed"
        );

        let deadline = Instant::now() + Duration::from_secs(60);
        let published = loop {
            if let SamplingState::Running { tick: Some(tick) } = metrics.state() {
                break tick;
            }
            assert!(
                Instant::now() < deadline,
                "the loop did not publish a tick within 60 s"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        assert!(published.sequence >= 1);
    }
}
