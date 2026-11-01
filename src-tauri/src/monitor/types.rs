//! The shapes one telemetry tick publishes: per-process rows, system totals,
//! and how the GPU figures relate to them.
//!
//! Two rules carried over from the plan govern every field here. A metric the
//! sampler could not read is `None`, never a zero that looks like an idle
//! process (D2). And because GPU engines tick slower than processes, a row can
//! carry a GPU figure from an earlier tick — so the tick says which sequence
//! refreshed them, and the UI can tell a stale number from a fresh one.
//!
//! The system totals are sums over the processes this sampler is allowed to
//! read. Windows hides the path of a chunk of every machine's process list, and
//! those processes report nothing here, so the disk and GPU totals are a floor
//! rather than a census; `unmeasured_processes` is how a reader knows.

use serde::Serialize;

use super::gpu::{GpuOutcome, GpuReport};
use super::process::ProcessSnapshot;

/// One process with its metrics for this tick.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    pub process: ProcessSnapshot,
    /// Sum over this process's sampled GPU engines, from the most recent GPU
    /// tick. `None` covers both "no engine reported this process" and "the GPU
    /// sampler has not produced a report yet"; the tick's own GPU state says
    /// which of the two it is.
    pub gpu_percent: Option<f64>,
}

/// What the GPU sampler can say about this machine right now.
///
/// Serialized as one tagged union: `kind` says which state it is, and a reader
/// never has to infer "warming" from a missing field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum GpuTelemetry {
    /// The counter is armed and waiting for its first full interval.
    Warming,
    /// Engines were read. `unparsed` counts instance names the parser could
    /// not read — the signal that the name format changed.
    Sampling {
        engines_sampled: usize,
        engines_unsampled: usize,
        unparsed: usize,
    },
    /// This machine has no `\GPU Engine`. The reason is shown, and the sampler
    /// will ask again only after its long wait.
    Unavailable { reason: String },
}

/// Machine-wide figures for one tick.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSummary {
    /// The whole machine, from `sysinfo`, which measures it directly. The
    /// per-process CPU figures do not add up to this: they cover only the
    /// processes whose handle the sampler could open.
    pub cpu_percent: Option<f64>,
    pub disk_read_bytes_per_sec: Option<f64>,
    pub disk_write_bytes_per_sec: Option<f64>,
    /// The sum over every sampled GPU engine of every process, so it can pass
    /// 100 % — a process uses several engines at once. That is the number the
    /// per-process column adds up to, not a single engine's utilisation.
    pub gpu_percent: Option<f64>,
    pub process_count: usize,
    /// Processes whose CPU and disk Windows would not give us. On a run's very
    /// first tick this is every process, because that tick has no window to
    /// measure rates over yet.
    pub unmeasured_processes: usize,
}

/// Everything one tick produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tick {
    /// Counts process ticks from 1, so a reader can tell two ticks apart.
    pub sequence: u64,
    pub processes: Vec<ProcessMetrics>,
    pub summary: SystemSummary,
    pub gpu: GpuTelemetry,
    /// The `sequence` of the tick whose GPU report filled these rows. `None`
    /// until the first GPU report arrives.
    pub gpu_sequence: Option<u64>,
}

/// Attaches a GPU report's per-process totals to fresh process snapshots.
pub fn merge(processes: Vec<ProcessSnapshot>, gpu: Option<&GpuReport>) -> Vec<ProcessMetrics> {
    processes
        .into_iter()
        .map(|process| {
            let gpu_percent = gpu.and_then(|report| {
                report
                    .processes
                    .iter()
                    .find(|row| row.pid == process.pid)
                    .and_then(|row| row.percent)
            });
            ProcessMetrics {
                process,
                gpu_percent,
            }
        })
        .collect()
}

/// Adds up what the rows measured. A total over no measurements is `None`.
pub fn summarise(rows: &[ProcessMetrics], cpu_percent: Option<f64>) -> SystemSummary {
    let mut summary = SystemSummary {
        cpu_percent,
        disk_read_bytes_per_sec: None,
        disk_write_bytes_per_sec: None,
        gpu_percent: None,
        process_count: rows.len(),
        unmeasured_processes: 0,
    };

    for row in rows {
        if row.process.cpu_percent.is_none() {
            summary.unmeasured_processes += 1;
        }

        if let Some(read) = row.process.disk_read_bytes_per_sec {
            *summary.disk_read_bytes_per_sec.get_or_insert(0.0) += read;
        }

        if let Some(write) = row.process.disk_write_bytes_per_sec {
            *summary.disk_write_bytes_per_sec.get_or_insert(0.0) += write;
        }

        if let Some(gpu) = row.gpu_percent {
            *summary.gpu_percent.get_or_insert(0.0) += gpu;
        }
    }

    summary
}

/// The published GPU state, from what the sampler just returned.
pub fn telemetry(outcome: &GpuOutcome) -> GpuTelemetry {
    match outcome {
        GpuOutcome::Warming => GpuTelemetry::Warming,
        GpuOutcome::Unavailable { reason } => GpuTelemetry::Unavailable {
            reason: reason.clone(),
        },
        GpuOutcome::Sampled { report } => GpuTelemetry::Sampling {
            engines_sampled: report.processes.iter().map(|row| row.engines.len()).sum(),
            engines_unsampled: report
                .processes
                .iter()
                .map(|row| row.unsampled_engines)
                .sum(),
            unparsed: report.unparsed.len(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::gpu::{EngineUsage, GpuEngine, ProcessGpuUsage};
    use crate::monitor::process::SignatureStatus;

    fn row(
        pid: u32,
        cpu: Option<f32>,
        read: Option<f64>,
        write: Option<f64>,
        gpu: Option<f64>,
    ) -> ProcessMetrics {
        ProcessMetrics {
            process: ProcessSnapshot {
                pid,
                parent_pid: None,
                name: format!("process-{pid}"),
                exe_path: Some(format!(r"C:\somewhere\process-{pid}.exe")),
                publisher: None,
                signature: SignatureStatus::Trusted,
                signature_reason: None,
                session_id: Some(1),
                window_title: None,
                start_time: 0,
                memory_bytes: 1_024,
                cpu_percent: cpu,
                disk_read_bytes_per_sec: read,
                disk_write_bytes_per_sec: write,
                critical_reason: None,
                selectable: true,
            },
            gpu_percent: gpu,
        }
    }

    fn engine_row(
        pid: u32,
        percent: Option<f64>,
        engines: usize,
        unsampled: usize,
    ) -> ProcessGpuUsage {
        ProcessGpuUsage {
            pid,
            percent,
            engines: (0..engines)
                .map(|index| EngineUsage {
                    engine: GpuEngine {
                        pid,
                        luid: "0x00000000_0x0000CD43".to_string(),
                        physical_adapter: 0,
                        engine_index: index as u32,
                        engine_type: "3D".to_string(),
                    },
                    percent: percent.unwrap_or_default() / engines.max(1) as f64,
                })
                .collect(),
            unsampled_engines: unsampled,
        }
    }

    #[test]
    fn totals_add_up_every_process_the_sampler_could_read() {
        let rows = [
            row(1, Some(10.0), Some(1_000.0), Some(500.0), None),
            row(2, Some(5.0), Some(250.0), None, None),
        ];

        let summary = summarise(&rows, Some(37.5));

        assert_eq!(summary.cpu_percent, Some(37.5));
        assert_eq!(summary.disk_read_bytes_per_sec, Some(1_250.0));
        assert_eq!(summary.disk_write_bytes_per_sec, Some(500.0));
        assert_eq!(summary.process_count, 2);
    }

    #[test]
    fn a_process_the_system_would_not_describe_is_counted_not_guessed() {
        let rows = [
            row(1, Some(10.0), Some(1_000.0), Some(500.0), None),
            row(2, None, None, None, None),
            row(3, None, None, None, None),
        ];

        let summary = summarise(&rows, Some(20.0));

        assert_eq!(summary.unmeasured_processes, 2);
        assert_eq!(summary.process_count, 3);
    }

    #[test]
    fn a_total_over_no_measurements_is_nothing_rather_than_zero() {
        let rows = [row(1, None, None, None, None)];

        let summary = summarise(&rows, None);

        assert_eq!(summary.disk_read_bytes_per_sec, None);
        assert_eq!(summary.disk_write_bytes_per_sec, None);
        assert_eq!(summary.gpu_percent, None);
    }

    #[test]
    fn the_gpu_total_adds_every_engine_of_every_process() {
        let rows = [
            row(1, Some(1.0), None, None, Some(12.5)),
            row(2, Some(1.0), None, None, Some(7.5)),
        ];

        assert_eq!(summarise(&rows, None).gpu_percent, Some(20.0));
    }

    #[test]
    fn merging_marks_an_idle_engine_zero_and_an_unmeasured_one_nothing() {
        // A PID with a sampled 0.0 is idle. A PID the report never mentioned
        // was not measured. Showing both as 0.0 would invent an idle GPU.
        let report = GpuReport {
            processes: vec![engine_row(2, Some(0.0), 1, 0), engine_row(3, None, 0, 2)],
            unparsed: Vec::new(),
        };
        let snapshots: Vec<ProcessSnapshot> = (1..=4)
            .map(|pid| row(pid, Some(1.0), None, None, None).process)
            .collect();

        let merged = merge(snapshots, Some(&report));

        assert_eq!(
            merged
                .iter()
                .find(|r| r.process.pid == 2)
                .unwrap()
                .gpu_percent,
            Some(0.0)
        );
        assert_eq!(
            merged
                .iter()
                .find(|r| r.process.pid == 1)
                .unwrap()
                .gpu_percent,
            None
        );
        assert_eq!(
            merged
                .iter()
                .find(|r| r.process.pid == 3)
                .unwrap()
                .gpu_percent,
            None
        );
    }

    #[test]
    fn merging_without_a_report_leaves_every_gpu_figure_unmeasured() {
        let snapshots: Vec<ProcessSnapshot> = (1..=3)
            .map(|pid| row(pid, Some(1.0), None, None, None).process)
            .collect();

        assert!(
            merge(snapshots, None)
                .iter()
                .all(|metrics| metrics.gpu_percent.is_none())
        );
    }

    #[test]
    fn the_telemetry_counts_engines_from_the_report_it_was_handed() {
        let report = GpuReport {
            processes: vec![engine_row(2, Some(4.0), 2, 1)],
            unparsed: vec!["pid_??_luid_broken".to_string()],
        };

        assert_eq!(
            telemetry(&GpuOutcome::Sampled { report }),
            GpuTelemetry::Sampling {
                engines_sampled: 2,
                engines_unsampled: 1,
                unparsed: 1,
            }
        );
    }

    #[test]
    fn the_telemetry_passes_warming_and_the_reason_through() {
        assert_eq!(telemetry(&GpuOutcome::Warming), GpuTelemetry::Warming);
        assert_eq!(
            telemetry(&GpuOutcome::Unavailable {
                reason: "no engine".to_string()
            }),
            GpuTelemetry::Unavailable {
                reason: "no engine".to_string()
            }
        );
    }

    #[test]
    fn a_tick_serializes_with_the_names_the_ui_reads() {
        let rows = vec![row(4712, Some(12.5), Some(1_000.0), None, Some(3.0))];
        let tick = Tick {
            sequence: 7,
            processes: rows.clone(),
            summary: summarise(&rows, Some(41.0)),
            gpu: GpuTelemetry::Sampling {
                engines_sampled: 2,
                engines_unsampled: 0,
                unparsed: 0,
            },
            gpu_sequence: Some(5),
        };

        let json = serde_json::to_value(&tick).expect("a tick serializes");

        assert_eq!(json["sequence"], 7);
        assert_eq!(json["gpuSequence"], 5);
        assert_eq!(json["summary"]["cpuPercent"], 41.0);
        assert_eq!(json["summary"]["processCount"], 1);
        assert_eq!(json["summary"]["unmeasuredProcesses"], 0);

        let process = &json["processes"][0];
        assert_eq!(process["process"]["pid"], 4712);
        assert_eq!(process["process"]["cpuPercent"], 12.5);
        assert_eq!(
            process["process"]["diskWriteBytesPerSec"],
            serde_json::Value::Null
        );
        assert_eq!(
            process["process"]["criticalReason"],
            serde_json::Value::Null
        );
        assert_eq!(process["gpuPercent"], 3.0);
    }

    #[test]
    fn the_gpu_state_serializes_as_one_union_the_ui_can_switch_on() {
        // Every state has to be distinguishable from JSON alone, or the UI cannot
        // tell "warming" from "this machine has no GPU counter".
        let warming = serde_json::to_value(GpuTelemetry::Warming).expect("warming");
        let sampling = serde_json::to_value(GpuTelemetry::Sampling {
            engines_sampled: 307,
            engines_unsampled: 11,
            unparsed: 0,
        })
        .expect("sampling");
        let unavailable = serde_json::to_value(GpuTelemetry::Unavailable {
            reason: "no \\GPU Engine object".to_string(),
        })
        .expect("unavailable");

        assert_eq!(warming["kind"], "warming");
        assert_eq!(sampling["kind"], "sampling");
        assert_eq!(sampling["enginesSampled"], 307);
        assert_eq!(sampling["enginesUnsampled"], 11);
        assert_eq!(sampling["unparsed"], 0);
        assert_eq!(unavailable["kind"], "unavailable");
        assert_eq!(unavailable["reason"], "no \\GPU Engine object");
    }
}
