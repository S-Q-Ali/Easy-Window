//! The tick loop: what the samplers produce, folded into the snapshots the UI
//! reads.
//!
//! Two clocks run here. Process snapshots come every second and GPU engines
//! every two (D4), so a tick can carry GPU figures from an earlier tick — the
//! published stamp is what keeps that honest rather than hidden. The scheduler
//! owns the samplers, and with them the signature cache, so exactly one cache
//! is shared by everything that asks whether a binary is signed.
//!
//! The loop writes into a [`SharedSnapshot`] and readers only ever take the
//! newest whole tick. That is why sampling can be slow without the UI waiting
//! on it: a cold first tick costs seconds, because verifying every binary for
//! the first time does, and a reader during that window gets `None` — "not
//! measured yet" — rather than a stale or partial table.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::gpu::{GpuOutcome, GpuReport, GpuSampler};
use super::process::{ProcessSampler, SignatureCache};
use super::types::{self, GpuTelemetry, Tick};

/// A process snapshot per second is enough to see a process start and stop, and
/// keeps the sampler's own cost well inside the idle budget. Public because the
/// overhead harness must sample at the loop's real cadence, not its own guess.
pub const DEFAULT_PROCESS_INTERVAL: Duration = Duration::from_secs(1);

/// GPU engines are the expensive part of the counter read — instance counts in
/// the hundreds — so they refresh slower than processes.
pub const DEFAULT_GPU_INTERVAL: Duration = Duration::from_secs(2);

/// Why the scheduler could not start.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("the machine could not be sampled: {0}")]
    Sampling(#[from] sysinfo::Error),
    #[error("the tick loop stopped unexpectedly")]
    Panicked,
}

/// The newest tick the loop has published, shared with whoever reads it.
#[derive(Clone, Default)]
pub struct SharedSnapshot(Arc<Mutex<Option<Tick>>>);

impl SharedSnapshot {
    fn publish(&self, tick: Tick) {
        *self.0.lock().expect("shared snapshot") = Some(tick);
    }

    /// The newest complete tick, or `None` before the first one is published.
    pub fn latest(&self) -> Option<Tick> {
        self.0.lock().expect("shared snapshot").clone()
    }
}

/// The GPU figures the current rows are allowed to carry, and the stamp saying
/// which tick measured them.
struct GpuState {
    report: Option<GpuReport>,
    sequence: Option<u64>,
    telemetry: GpuTelemetry,
}

impl GpuState {
    /// A state in which the counter is armed and nothing has been read yet.
    fn warming() -> Self {
        Self {
            report: None,
            sequence: None,
            telemetry: GpuTelemetry::Warming,
        }
    }

    /// Folds one GPU verdict in. A new report replaces the old one and is
    /// stamped with the tick that read it; warming leaves the previous report
    /// in place because those figures were really measured, just not this
    /// tick; a machine that stops answering loses the figures, because rows
    /// carrying numbers next to "this machine has no GPU counter" would be a
    /// contradiction the reader cannot resolve.
    fn record(&mut self, outcome: GpuOutcome, sequence: u64) {
        self.telemetry = types::telemetry(&outcome);

        match outcome {
            GpuOutcome::Sampled { report } => {
                self.report = Some(report);
                self.sequence = Some(sequence);
            }
            GpuOutcome::Warming => {}
            GpuOutcome::Unavailable { .. } => {
                self.report = None;
                self.sequence = None;
            }
        }
    }

    fn report(&self) -> Option<&GpuReport> {
        self.report.as_ref()
    }

    fn sequence(&self) -> Option<u64> {
        self.sequence
    }

    fn telemetry(&self) -> GpuTelemetry {
        self.telemetry.clone()
    }
}

/// Whether this tick should collect the GPU counter, given when it last did.
fn gpu_is_due(last: Option<Instant>, now: Instant, interval: Duration) -> bool {
    match last {
        Some(last) => now.duration_since(last) >= interval,
        None => true,
    }
}

/// Drives the process and GPU samplers and publishes one tick per process
/// snapshot.
pub struct Scheduler {
    processes: ProcessSampler,
    gpu: GpuSampler,
    gpu_state: GpuState,
    shared: SharedSnapshot,
    sequence: u64,
    last_gpu: Option<Instant>,
    process_interval: Duration,
    gpu_interval: Duration,
}

impl Scheduler {
    pub fn new() -> Result<Self, SchedulerError> {
        Self::with_intervals(DEFAULT_PROCESS_INTERVAL, DEFAULT_GPU_INTERVAL)
    }

    /// A scheduler with chosen clocks, so a test or a settings value can pick
    /// its own cadence.
    pub fn with_intervals(
        process_interval: Duration,
        gpu_interval: Duration,
    ) -> Result<Self, SchedulerError> {
        // One cache for the run: the scheduler is what owns it, and everything
        // that later asks about a signature goes through this sampler.
        let processes = ProcessSampler::with_cache(SignatureCache::default())?;

        Ok(Self {
            processes,
            gpu: GpuSampler::new(),
            gpu_state: GpuState::warming(),
            shared: SharedSnapshot::default(),
            sequence: 0,
            last_gpu: None,
            process_interval,
            gpu_interval,
        })
    }

    /// Samples once, assembles the tick, and publishes it.
    pub fn tick(&mut self) -> Tick {
        self.sequence += 1;
        let snapshots = self.processes.snapshot();
        let system_cpu = self.processes.system_cpu_percent();

        let now = Instant::now();
        if gpu_is_due(self.last_gpu, now, self.gpu_interval) {
            self.last_gpu = Some(now);
            let outcome = self.gpu.tick();
            self.gpu_state.record(outcome, self.sequence);
        }

        let processes = types::merge(snapshots, self.gpu_state.report());
        let summary = types::summarise(&processes, system_cpu);
        let tick = Tick {
            sequence: self.sequence,
            processes,
            gpu: self.gpu_state.telemetry(),
            gpu_sequence: self.gpu_state.sequence(),
            summary,
        };

        self.shared.publish(tick.clone());
        tick
    }

    /// The newest published tick, from this scheduler's own slot.
    pub fn latest(&self) -> Option<Tick> {
        self.shared.latest()
    }

    /// Runs the loop on its own thread until the returned handle is stopped,
    /// handing every tick it publishes to `on_tick` first. That is how the app
    /// gets a tick to emit without a second reader polling the shared slot and
    /// missing the ones it did not ask for.
    pub fn spawn(mut self, on_tick: impl Fn(&Tick) + Send + 'static) -> RunningScheduler {
        let (stop, control) = mpsc::channel();
        let shared = self.shared.clone();
        let process_interval = self.process_interval;

        let join = thread::spawn(move || {
            loop {
                let tick = self.tick();
                on_tick(&tick);
                match control.recv_timeout(process_interval) {
                    Err(RecvTimeoutError::Timeout) => {}
                    // Either way, stop ticking: a message asked, or the handle was
                    // dropped and nobody is listening anymore.
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        RunningScheduler { stop, join, shared }
    }
}

/// A tick loop running on its own thread. Dropping this handle closes the
/// control channel, which is how the loop learns to stop.
pub struct RunningScheduler {
    stop: Sender<()>,
    join: JoinHandle<()>,
    shared: SharedSnapshot,
}

impl RunningScheduler {
    pub fn latest(&self) -> Option<Tick> {
        self.shared.latest()
    }

    /// Asks the loop to finish, waits for it, and hands back the shared slot
    /// with whatever the last tick published.
    pub fn stop(self) -> Result<SharedSnapshot, SchedulerError> {
        let Self { stop, join, shared } = self;
        drop(stop);

        join.join()
            .map_err(|_| SchedulerError::Panicked)
            .map(|_| shared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::gpu::{EngineUsage, GpuEngine, GpuReport, ProcessGpuUsage};

    fn report(pid: u32, percent: f64) -> GpuReport {
        GpuReport {
            processes: vec![ProcessGpuUsage {
                pid,
                percent: Some(percent),
                engines: vec![EngineUsage {
                    engine: GpuEngine {
                        pid,
                        luid: "0x00000000_0x0000CD43".to_string(),
                        physical_adapter: 0,
                        engine_index: 0,
                        engine_type: "3D".to_string(),
                    },
                    percent,
                }],
                unsampled_engines: 0,
            }],
            unparsed: Vec::new(),
        }
    }

    #[test]
    fn a_sampled_verdict_becomes_the_gpu_state_the_next_rows_carry() {
        let mut state = GpuState::warming();

        state.record(
            GpuOutcome::Sampled {
                report: report(7, 12.5),
            },
            5,
        );

        assert_eq!(state.sequence(), Some(5));
        assert_eq!(state.report().map(|r| r.processes.len()), Some(1));
        assert_eq!(
            state.telemetry(),
            GpuTelemetry::Sampling {
                engines_sampled: 1,
                engines_unsampled: 0,
                unparsed: 0,
            }
        );
    }

    #[test]
    fn warming_keeps_the_last_report_and_says_which_tick_measured_it() {
        let mut state = GpuState::warming();
        state.record(
            GpuOutcome::Sampled {
                report: report(7, 12.5),
            },
            5,
        );

        state.record(GpuOutcome::Warming, 6);

        assert_eq!(state.sequence(), Some(5), "the figures are still tick 5's");
        assert!(state.report().is_some());
    }

    #[test]
    fn an_unavailable_machine_stops_claiming_gpu_figures() {
        let mut state = GpuState::warming();
        state.record(
            GpuOutcome::Sampled {
                report: report(7, 12.5),
            },
            5,
        );

        state.record(
            GpuOutcome::Unavailable {
                reason: "no GPU Engine object".to_string(),
            },
            6,
        );

        assert_eq!(state.sequence(), None);
        assert!(state.report().is_none());
        assert_eq!(
            state.telemetry(),
            GpuTelemetry::Unavailable {
                reason: "no GPU Engine object".to_string()
            }
        );
    }

    #[test]
    fn nothing_is_claimed_before_the_first_sampled_verdict() {
        let state = GpuState::warming();

        assert_eq!(state.sequence(), None);
        assert!(state.report().is_none());
        assert_eq!(state.telemetry(), GpuTelemetry::Warming);
    }

    #[test]
    fn the_gpu_counter_waits_its_own_interval_between_ticks() {
        let now = Instant::now();
        let interval = Duration::from_secs(2);

        assert!(
            gpu_is_due(None, now, interval),
            "a counter that has never collected is due now"
        );
        assert!(
            !gpu_is_due(Some(now - Duration::from_secs(1)), now, interval),
            "a counter that collected a second ago is not due"
        );
        assert!(
            gpu_is_due(Some(now - Duration::from_secs(2)), now, interval),
            "a counter that collected exactly an interval ago is due"
        );
    }

    #[test]
    fn the_loop_hands_every_published_tick_to_its_sink() {
        // The app emits each tick to the window from inside the loop, so the
        // loop has to be able to hand the tick it just published to a consumer
        // without that consumer reaching into the shared slot and racing itself.
        let (sender, receiver) = mpsc::channel();
        let running =
            Scheduler::with_intervals(Duration::from_millis(200), Duration::from_millis(1_400))
                .expect("scheduler")
                .spawn(move |tick| {
                    let _ = sender.send(tick.sequence);
                });

        let first = receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("a tick reached the sink");
        let second = receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("a second tick reached the sink");
        assert!(second > first, "the sink sees the loop advance");

        let shared = running.stop().expect("the loop joins");
        let latest = shared.latest().expect("the last tick is published");
        assert!(latest.sequence >= second);
    }

    #[test]
    fn the_loop_publishes_numbered_ticks_and_stops_when_told() {
        let mut scheduler =
            Scheduler::with_intervals(Duration::from_millis(100), Duration::from_millis(100))
                .expect("scheduler");

        let first = scheduler.tick();
        assert_eq!(first.sequence, 1);
        assert_eq!(scheduler.latest().map(|tick| tick.sequence), Some(1));
        assert_eq!(first.summary.process_count, first.processes.len());

        let second = scheduler.tick();
        assert_eq!(second.sequence, 2);
        assert_eq!(scheduler.latest().map(|tick| tick.sequence), Some(2));

        let running = scheduler.spawn(|_| {});
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if running.latest().is_some_and(|tick| tick.sequence >= 4) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the loop did not publish four ticks within 30 s"
            );
            std::thread::sleep(Duration::from_millis(100));
        }

        let shared = running.stop().expect("the tick loop stopped cleanly");
        let last = shared
            .latest()
            .expect("a stopped loop still has its last tick");
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(
            shared.latest().map(|tick| tick.sequence),
            Some(last.sequence),
            "no tick may be published after the loop is told to stop"
        );
    }

    #[test]
    fn a_tick_never_claims_gpu_figures_it_did_not_measure() {
        let mut scheduler =
            Scheduler::with_intervals(Duration::from_millis(50), Duration::from_millis(50))
                .expect("scheduler");

        for _ in 0..3 {
            let tick = scheduler.tick();

            match tick.gpu_sequence {
                Some(stamp) => assert!(
                    stamp <= tick.sequence,
                    "figures stamped {stamp} cannot post-date tick {}",
                    tick.sequence
                ),
                None => {
                    assert!(
                        tick.processes.iter().all(|row| row.gpu_percent.is_none()),
                        "an unmeasured GPU must leave every row without a figure"
                    );
                    assert_eq!(tick.summary.gpu_percent, None);
                }
            }
        }
    }

    #[test]
    fn a_gpu_report_is_stamped_with_the_tick_that_read_it_and_carried_after() {
        // The counter path is real, so a machine with a WDDM driver answers
        // within two intervals; a machine without one has nothing to stamp.
        let mut scheduler =
            Scheduler::with_intervals(Duration::from_millis(200), Duration::from_millis(1_400))
                .expect("scheduler");

        let armed = scheduler.tick();
        if let GpuTelemetry::Unavailable { .. } = &armed.gpu {
            return;
        }

        // Windows recomputes performance data about once per second.
        std::thread::sleep(Duration::from_millis(1_500));
        let read = scheduler.tick();
        match read.gpu {
            GpuTelemetry::Sampling { .. } => {
                assert_eq!(read.gpu_sequence, Some(read.sequence), "this tick read it")
            }
            other => panic!("a counter given an interval and a half must sample, got {other:?}"),
        }

        let carried = scheduler.tick();
        assert_eq!(
            carried.sequence,
            read.sequence + 1,
            "the next tick is one later"
        );
        // Two legal outcomes, and the difference is real rather than a flake to
        // paper over: the figures were carried from the tick that read them, or
        // that tick took longer than the GPU interval and this one read the
        // counter again — the measured mean tick is 372 ms of a 1 s interval and
        // the worst ran 4.07 s, so a fixed wall-time assumption here would be the
        // bug. What must never happen is figures stamped by a tick that did not
        // produce them, or a stamp that disappears while the figures stay.
        let stamp = carried
            .gpu_sequence
            .expect("figures a tick measured are not un-measured by the next one");
        assert!(
            stamp == read.sequence || stamp == carried.sequence,
            "tick {} carries GPU figures stamped {stamp}, which no recent tick read",
            carried.sequence
        );
    }

    // The loop's `thread::spawn` is the real check, but this states the
    // requirement where a reader can see it.
    const _: fn() = || {
        fn assert_send<T: Send>() {}
        assert_send::<Scheduler>();
        assert_send::<RunningScheduler>();
        fn assert_sync<T: Sync>() {}
        assert_sync::<SharedSnapshot>();
    };
}
