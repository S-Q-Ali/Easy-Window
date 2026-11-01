//! Manual PDH probe: what a warm tick costs, what a wildcard counter returns,
//! and what the availability ladder says about a missing object.
//!
//! Run with:
//!
//! ```text
//! cargo run --manifest-path src-tauri/Cargo.toml --example pdh
//! ```

#[cfg(windows)]
fn main() {
    use std::time::{Duration, Instant};

    use easy_window_lib::monitor::pdh::{PdhError, Query, object_instances};

    let query = match Query::open() {
        Ok(query) => query,
        Err(error) => {
            eprintln!("could not open a PDH query: {error}");
            std::process::exit(1);
        }
    };

    let paths = [
        r"\Process(_Total)\% Processor Time",
        r"\Process(*)\% Processor Time",
    ];
    let (total, wildcard) = match (query.add(paths[0]), query.add(paths[1])) {
        (Ok(total), Ok(wildcard)) => (total, wildcard),
        (Err(error), _) | (_, Err(error)) => {
            eprintln!("could not add a counter: {error}");
            std::process::exit(1);
        }
    };

    // Five one-second ticks. The first read of a fresh wildcard counter can
    // answer "no data yet" because PDH builds the instance list on that call;
    // the ticks after it are what a running sampler sees.
    for tick in 1..=5 {
        if tick > 1 {
            std::thread::sleep(Duration::from_secs(1));
        }

        let started = Instant::now();
        if let Err(error) = query.collect() {
            println!("tick {tick}: collect: {error}");
        }
        let collect = started.elapsed();

        let value = match query.value(&total) {
            Ok(Some(percent)) => format!("{percent:.1}%"),
            Ok(None) => "not sampled".to_string(),
            Err(error) => format!("{error}"),
        };

        let started = Instant::now();
        let samples = query.samples(&wildcard);
        let read = started.elapsed();

        match samples {
            Ok(samples) => {
                let sampled = samples
                    .iter()
                    .filter(|sample| sample.value.is_some())
                    .count();
                let mut statuses: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                for sample in &samples {
                    *statuses.entry(format!("{:?}", sample.status)).or_insert(0) += 1;
                }
                println!(
                    "tick {tick}: {} instances, {sampled} with a value, {statuses:?}, _Total {value}, collect {collect:?}, read {read:?}",
                    samples.len()
                );
            }
            Err(PdhError::NoData) => {
                println!("tick {tick}: no array yet, _Total {value}, collect {collect:?}");
            }
            Err(error) => println!("tick {tick}: array read failed: {error}"),
        }
    }

    // The availability ladder, and what one refresh costs: the object list is
    // cached inside PDH, so this is a probe-once call, not a per-tick one.
    let started = Instant::now();
    match object_instances("GPU Engine") {
        Ok(instances) => println!(
            "\\GPU Engine: {} instances in {:?}, first {:?}",
            instances.len(),
            started.elapsed(),
            instances.first()
        ),
        Err(error) => println!(
            "\\GPU Engine: unavailable ({error}) after {time:?}",
            time = started.elapsed()
        ),
    }

    match object_instances("Easy Window Not A Performance Object") {
        Ok(instances) => println!("unknown object listed {} entries", instances.len()),
        Err(error) => println!("unknown object: {error}"),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Easy Window only runs on Windows.");
}
