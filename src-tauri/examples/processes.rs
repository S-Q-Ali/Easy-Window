//! Manual process snapshot dump for comparing with Task Manager.
//!
//! Run with:
//!
//! ```text
//! cargo run --manifest-path src-tauri/Cargo.toml --example processes
//! ```

#[cfg(windows)]
fn main() {
    use std::time::{Duration, Instant};

    use easy_window_lib::monitor::process::ProcessSampler;

    let mut sampler = match ProcessSampler::new() {
        Ok(sampler) => sampler,
        Err(error) => {
            eprintln!("could not start the process sampler: {error}");
            std::process::exit(1);
        }
    };

    let started = Instant::now();
    let first = sampler.snapshot();
    let cold = started.elapsed();

    let started = Instant::now();
    sampler.snapshot();
    let warm = started.elapsed();

    // CPU and disk figures are deltas between two refreshes, so the table reads
    // from a snapshot taken one second later — the cadence the UI will use.
    std::thread::sleep(Duration::from_secs(1));
    let sampled = sampler.snapshot();

    println!(
        "{:<7} {:<7} {:<5} {:>6} {:>9} {:>11} {:>11} {:<30} {:<9} {:<30} WINDOW TITLE / PATH",
        "PID", "PPID", "SESS", "CPU%", "MEM", "READ/s", "WRITE/s", "NAME", "SIGNATURE", "PUBLISHER"
    );

    for process in &sampled {
        let parent = process
            .parent_pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string());
        let session = process
            .session_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let publisher = process.publisher.as_deref().unwrap_or("-");
        let critical = process.critical_reason.map(|reason| reason.label());

        println!(
            "{:<7} {:<7} {:<5} {:>6} {:>9} {:>11} {:>11} {:<30} {:<9} {:<30} {}",
            process.pid,
            parent,
            session,
            process
                .cpu_percent
                .map(|cpu| format!("{cpu:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            bytes(process.memory_bytes as f64),
            rate(process.disk_read_bytes_per_sec),
            rate(process.disk_write_bytes_per_sec),
            truncate(&process.name, 30),
            format!("{:?}", process.signature),
            truncate(publisher, 30),
            critical.unwrap_or("")
        );

        if let Some(path) = &process.exe_path {
            println!("{:>60}{}", "", path);
        }
        if let Some(title) = &process.window_title {
            println!("{:>60}window: {}", "", truncate(title, 90));
        }
        if let Some(reason) = &process.signature_reason {
            println!("{:>60}reason: {}", "", reason);
        }
    }

    println!("\n{} processes", sampled.len());
    println!(
        "cold snapshot {cold:?} ({} processes), warm snapshot {warm:?}, sampled over the last second: {} processes",
        first.len(),
        sampled.len()
    );

    fn truncate(value: &str, max: usize) -> String {
        if value.chars().count() <= max {
            value.to_string()
        } else {
            let mut truncated: String = value.chars().take(max.saturating_sub(1)).collect();
            truncated.push('…');
            truncated
        }
    }

    fn bytes(value: f64) -> String {
        const KILOBYTE: f64 = 1024.0;
        if value < KILOBYTE {
            format!("{value:.0}B")
        } else if value < KILOBYTE * KILOBYTE {
            format!("{:.1}KB", value / KILOBYTE)
        } else {
            format!("{:.1}MB", value / (KILOBYTE * KILOBYTE))
        }
    }

    fn rate(bytes_per_second: Option<f64>) -> String {
        bytes_per_second
            .map(|value| format!("{}/s", bytes(value)))
            .unwrap_or_else(|| "-".to_string())
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Easy Window only runs on Windows.");
}
