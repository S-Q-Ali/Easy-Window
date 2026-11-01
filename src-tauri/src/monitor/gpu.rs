//! Turns `\GPU Engine(*)` instance names into per-process GPU usage.
//!
//! PDH attributes GPU time to a process only through the instance name —
//! `pid_<pid>_luid_<hi>_<lo>_phys_<n>_eng_<n>_engtype_<type>` is the whole
//! API — so this parser is the boundary where that string becomes data. Two
//! measured facts shape it: on this machine's 287 instances the `engtype_`
//! value is empty 60 times and contains a space 20 more (`GDI Render`), so
//! the type is kept verbatim rather than matched against a list; and one PID
//! spreads across several LUIDs, so a process's usage is the sum over every
//! engine row that carries it.
//!
//! The aggregation follows the Task Manager model the plan decided on: sum
//! every engine of a PID over the sample interval. An engine PDH did not
//! sample adds nothing to the sum and is counted, so a total built from
//! partial data stays distinguishable from an idle GPU.

use super::pdh::Sample;

/// One engine of one process, as its instance name describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuEngine {
    pub pid: u32,
    /// The Locally Unique ID of the adapter, kept verbatim
    /// (`0x00000000_0x0000CD43`). Case varies with the Windows version.
    pub luid: String,
    pub physical_adapter: u32,
    pub engine_index: u32,
    /// Everything after `engtype_`, verbatim: `3D`, `GDI Render`, or empty
    /// when the driver reports no type. Never a fixed enum.
    pub engine_type: String,
}

/// One engine and the percentage it used over the interval.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineUsage {
    pub engine: GpuEngine,
    pub percent: f64,
}

/// The GPU usage of one process: its total and the engines that made it up.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessGpuUsage {
    pub pid: u32,
    /// The sum over [`Self::engines`]. `None` when every engine of this
    /// process went unsampled — a process with an unsampled GPU is not a
    /// process with an idle GPU.
    pub percent: Option<f64>,
    /// The engines that were sampled, ordered by LUID, adapter and index.
    pub engines: Vec<EngineUsage>,
    /// Engines this process owns that PDH did not sample this tick.
    pub unsampled_engines: usize,
}

/// What one tick's GPU engine samples say, plus what the parser could not
/// read. A non-empty [`Self::unparsed`] is how a renamed instance format
/// reaches the logs instead of silently becoming zero usage.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuReport {
    /// Ordered by percentage descending, then PID ascending.
    pub processes: Vec<ProcessGpuUsage>,
    pub unparsed: Vec<String>,
}

/// Reads the parts out of a `\GPU Engine` instance name, or `None` when the
/// name is not this shape — `\GPU Process Memory` instances, for example,
/// end at `_phys_0` with no engine at all.
pub fn parse_engine(name: &str) -> Option<GpuEngine> {
    // Every token below is anchored where the previous one ended, so an
    // engine type that itself contains `engtype_` or `_eng_` cannot move the
    // boundaries — it is only ever the verbatim tail.
    let rest = name.strip_prefix("pid_")?;
    let (pid, rest) = take_number(rest)?;
    let rest = rest.strip_prefix("luid_")?;
    let (luid, rest) = rest.split_once("_phys_")?;
    if luid.is_empty() {
        return None;
    }
    let (physical_adapter, rest) = take_number(rest)?;
    let rest = rest.strip_prefix("eng_")?;
    let (engine_index, rest) = take_number(rest)?;
    let engine_type = rest.strip_prefix("engtype_")?;

    Some(GpuEngine {
        pid,
        luid: luid.to_string(),
        physical_adapter,
        engine_index,
        engine_type: engine_type.to_string(),
    })
}

/// Reads the leading run of digits up to the next `_`, and returns the value
/// with the remainder past that separator.
fn take_number(rest: &str) -> Option<(u32, &str)> {
    let (segment, rest) = rest.split_once('_')?;
    if segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((segment.parse().ok()?, rest))
}

/// Groups samples by PID and sums each process's sampled engines.
pub fn aggregate(samples: &[Sample]) -> GpuReport {
    struct Usage {
        sampled: Vec<EngineUsage>,
        unsampled: usize,
    }

    let mut by_pid: std::collections::BTreeMap<u32, Usage> = std::collections::BTreeMap::new();
    let mut unparsed = Vec::new();

    for sample in samples {
        let Some(engine) = parse_engine(&sample.instance) else {
            unparsed.push(sample.instance.clone());
            continue;
        };

        let usage = by_pid.entry(engine.pid).or_insert(Usage {
            sampled: Vec::new(),
            unsampled: 0,
        });
        match sample.value {
            Some(percent) => usage.sampled.push(EngineUsage { engine, percent }),
            None => usage.unsampled += 1,
        }
    }

    let mut processes: Vec<ProcessGpuUsage> = by_pid
        .into_iter()
        .map(|(pid, mut usage)| {
            // The tooltip breakdown reads in adapter order, not in whatever
            // order PDH happened to enumerate the instances.
            usage.sampled.sort_by(|a, b| {
                (
                    &a.engine.luid,
                    a.engine.physical_adapter,
                    a.engine.engine_index,
                )
                    .cmp(&(
                        &b.engine.luid,
                        b.engine.physical_adapter,
                        b.engine.engine_index,
                    ))
            });
            let percent = (!usage.sampled.is_empty())
                .then(|| usage.sampled.iter().map(|engine| engine.percent).sum());
            ProcessGpuUsage {
                pid,
                percent,
                engines: usage.sampled,
                unsampled_engines: usage.unsampled,
            }
        })
        .collect();

    // Busiest first; an unmeasured process sorts after every measured one.
    processes.sort_by(|a, b| {
        b.percent
            .unwrap_or(f64::MIN)
            .total_cmp(&a.percent.unwrap_or(f64::MIN))
            .then(a.pid.cmp(&b.pid))
    });

    GpuReport {
        processes,
        unparsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::pdh::{CounterStatus, sample_of};
    use windows::Win32::System::Performance::PDH_CSTATUS_VALID_DATA;

    /// A measured name from this machine turned into a sampled value.
    fn measured(name: &str, percent: f64) -> Sample {
        sample_of(name.to_string(), PDH_CSTATUS_VALID_DATA, percent)
    }

    /// A name PDH reported without sampling it this interval.
    fn unsampled(name: &str) -> Sample {
        Sample {
            instance: name.to_string(),
            status: CounterStatus::InvalidData,
            value: None,
        }
    }

    // Every shape the 287 live names on this machine take, captured 2026-11-01.
    // The empty type carries the 60 instances whose driver reports nothing,
    // `GDI Render` the 20 whose type contains a space, and the lowercase LUID
    // is the same name as an earlier Windows build reported it.
    const REAL_NAMES: &[&str] = &[
        "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D",
        "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_1_engtype_VideoDecode",
        "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_2_engtype_Copy",
        "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_3_engtype_VideoProcessing",
        "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_7_engtype_LegacyOverlay",
        "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_8_engtype_GDI Render",
        "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_10_engtype_",
        "pid_4_luid_0x00000000_0x0000D22B_phys_0_eng_1_engtype_Security",
        "pid_1076_luid_0x00000000_0x0000cd43_phys_0_eng_0_engtype_3D",
    ];

    #[test]
    fn every_captured_engine_name_parses() {
        for name in REAL_NAMES {
            assert!(
                parse_engine(name).is_some(),
                "a name this machine really reported must parse: {name}"
            );
        }
    }

    #[test]
    fn a_real_engine_name_parses_into_its_parts() {
        let engine =
            parse_engine("pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_1_engtype_VideoDecode")
                .expect("a measured instance name");

        assert_eq!(engine.pid, 1076);
        assert_eq!(engine.luid, "0x00000000_0x0000CD43");
        assert_eq!(engine.physical_adapter, 0);
        assert_eq!(engine.engine_index, 1);
        assert_eq!(engine.engine_type, "VideoDecode");
    }

    #[test]
    fn an_empty_engine_type_stays_empty() {
        // 60 of this machine's 287 instances end right after `engtype_`.
        let engine = parse_engine("pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_10_engtype_")
            .expect("a measured instance name");

        assert_eq!(engine.engine_index, 10);
        assert_eq!(engine.engine_type, "");
    }

    #[test]
    fn an_engine_type_with_a_space_is_kept_verbatim() {
        let engine =
            parse_engine("pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_8_engtype_GDI Render")
                .expect("a measured instance name");

        assert_eq!(engine.engine_type, "GDI Render");
    }

    #[test]
    fn an_engine_type_with_a_leading_digit_is_not_a_number() {
        let engine = parse_engine("pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D")
            .expect("a measured instance name");

        assert_eq!(engine.engine_type, "3D");
    }

    #[test]
    fn luid_case_is_not_assumed() {
        // The same shape arrives lowercase on some Windows builds.
        let upper =
            parse_engine("pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D").unwrap();
        let lower =
            parse_engine("pid_1076_luid_0x00000000_0x0000cd43_phys_0_eng_0_engtype_3D").unwrap();

        assert_eq!(upper.engine_index, lower.engine_index);
        assert_eq!(lower.luid, "0x00000000_0x0000cd43");
    }

    #[test]
    fn a_name_that_is_not_an_engine_is_rejected() {
        // `\GPU Process Memory` instances — the VRAM object, if it is ever
        // added — end at `_phys_0` with no engine and no type.
        assert_eq!(
            parse_engine("pid_1076_luid_0x00000000_0x0000CD43_phys_0"),
            None
        );
        assert_eq!(parse_engine("notepad.exe"), None);
        assert_eq!(parse_engine(""), None);
        // A PID that does not fit a u32 is not a PID Windows can report, so
        // it is a broken name rather than a process.
        assert_eq!(
            parse_engine("pid_99999999999_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D"),
            None
        );
    }

    #[test]
    fn every_engine_of_a_process_is_summed() {
        let report = aggregate(&[
            measured(
                "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D",
                12.5,
            ),
            measured(
                "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_2_engtype_Copy",
                2.25,
            ),
        ]);

        assert_eq!(report.processes.len(), 1);
        assert_eq!(report.processes[0].pid, 1076);
        assert_eq!(report.processes[0].percent, Some(14.75));
        assert_eq!(report.processes[0].engines.len(), 2);
    }

    #[test]
    fn engines_from_several_luids_belong_to_the_same_process() {
        // This machine runs three adapter contexts; a browser using two of
        // them is still one process with one usage number.
        let report = aggregate(&[
            measured(
                "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D",
                4.0,
            ),
            measured(
                "pid_1076_luid_0x00000000_0x0000D22B_phys_0_eng_0_engtype_3D",
                3.0,
            ),
        ]);

        assert_eq!(report.processes.len(), 1);
        assert_eq!(report.processes[0].percent, Some(7.0));
        assert_eq!(report.processes[0].engines.len(), 2);
    }

    #[test]
    fn an_unsampled_engine_adds_nothing_and_is_counted() {
        let report = aggregate(&[
            measured(
                "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D",
                5.0,
            ),
            unsampled("pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_10_engtype_"),
        ]);

        assert_eq!(report.processes[0].percent, Some(5.0));
        assert_eq!(report.processes[0].engines.len(), 1);
        assert_eq!(report.processes[0].unsampled_engines, 1);
    }

    #[test]
    fn a_process_whose_engines_are_all_unsampled_has_no_percentage() {
        // The honest answer here is "not measured", not zero — zero would
        // claim an idle GPU the sampler never saw.
        let report = aggregate(&[
            unsampled("pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D"),
            unsampled("pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_1_engtype_VideoDecode"),
        ]);

        assert_eq!(report.processes.len(), 1);
        assert_eq!(report.processes[0].percent, None);
        assert_eq!(report.processes[0].unsampled_engines, 2);
    }

    #[test]
    fn a_name_that_does_not_parse_is_reported_not_dropped() {
        let report = aggregate(&[
            measured(
                "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D",
                5.0,
            ),
            measured("pid_??_luid_broken", 1.0),
        ]);

        assert_eq!(report.processes.len(), 1);
        assert_eq!(report.unparsed, vec!["pid_??_luid_broken".to_string()]);
    }

    #[test]
    fn rows_are_ordered_by_usage_then_pid() {
        let report = aggregate(&[
            measured(
                "pid_4_luid_0x00000000_0x0000D22B_phys_0_eng_1_engtype_Security",
                1.0,
            ),
            measured(
                "pid_1076_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D",
                9.0,
            ),
            measured(
                "pid_11104_luid_0x00000000_0x0000CD43_phys_0_eng_0_engtype_3D",
                9.0,
            ),
        ]);

        let order: Vec<u32> = report.processes.iter().map(|row| row.pid).collect();
        assert_eq!(order, vec![1076, 11104, 4]);
    }

    #[test]
    fn an_empty_tick_is_an_empty_report() {
        assert_eq!(
            aggregate(&[]),
            GpuReport {
                processes: Vec::new(),
                unparsed: Vec::new()
            }
        );
    }
}
