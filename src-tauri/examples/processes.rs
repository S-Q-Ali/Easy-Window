//! Manual process snapshot dump for comparing with Task Manager.
//!
//! Run with:
//!
//! ```text
//! cargo run --manifest-path src-tauri/Cargo.toml --example processes
//! ```

#[cfg(windows)]
fn main() {
    use std::time::Instant;

    use easy_window_lib::monitor::process::ProcessSampler;

    let mut sampler = match ProcessSampler::new() {
        Ok(sampler) => sampler,
        Err(error) => {
            eprintln!("could not start the process sampler: {error}");
            std::process::exit(1);
        }
    };

    let started = Instant::now();
    let snapshot = sampler.snapshot();
    let cold = started.elapsed();

    let started = Instant::now();
    let second = sampler.snapshot();
    let warm = started.elapsed();

    println!(
        "{:<7} {:<7} {:<5} {:<30} {:<9} {:<30} WINDOW TITLE / PATH",
        "PID", "PPID", "SESS", "NAME", "SIGNATURE", "PUBLISHER"
    );

    for process in &snapshot {
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
            "{:<7} {:<7} {:<5} {:<30} {:<9} {:<30} {}",
            process.pid,
            parent,
            session,
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

    println!("\n{} processes", snapshot.len());
    println!(
        "cold snapshot {cold:?}, warm snapshot {warm:?}, {} processes on the second pass",
        second.len()
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
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Easy Window only runs on Windows.");
}
