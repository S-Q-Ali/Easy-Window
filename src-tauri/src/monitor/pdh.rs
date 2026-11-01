//! A narrow, safe wrapper over the Performance Data Helper (PDH) query API.
//!
//! PDH is here for one job: per-process GPU utilisation, which Windows publishes
//! through no other interface. CPU and disk come from `sysinfo`, so adding PDH
//! counters for them would create a second source for the same number.
//!
//! Two rules shape the API. A counter is added once and collected on every
//! tick — rebuilding counter handles per tick is the difference between
//! milliseconds and seconds — and every status PDH returns becomes a named
//! outcome, because a zero left in an unsampled slot is not a measurement and
//! must not be forwarded as one.

use std::marker::PhantomData;
use std::mem::size_of;

use windows::Win32::System::Performance::{
    PDH_CSTATUS_INVALID_DATA, PDH_CSTATUS_ITEM_NOT_VALIDATED, PDH_CSTATUS_NEW_DATA,
    PDH_CSTATUS_NO_COUNTER, PDH_CSTATUS_NO_INSTANCE, PDH_CSTATUS_NO_OBJECT, PDH_CSTATUS_VALID_DATA,
    PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
    PDH_INVALID_DATA, PDH_INVALID_PATH, PDH_MORE_DATA, PDH_NO_DATA, PDH_NO_MORE_DATA,
    PERF_DETAIL_WIZARD, PdhAddEnglishCounterW, PdhCollectQueryData, PdhEnumObjectItemsW,
    PdhEnumObjectsW, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::core::{Owned, PCWSTR, PWSTR};

/// How far a NUL terminator is looked for before the buffer is assumed broken.
const MAX_WIDE_UNITS: usize = 32_768;

/// How many sizing attempts one array read is allowed, because process
/// instances can appear between the call that measures the buffer and the call
/// that fills it.
const MAX_ARRAY_ATTEMPTS: u32 = 4;

/// What PDH says about one counter or one item of an array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterStatus {
    Valid,
    NewData,
    NoInstance,
    NoObject,
    NoCounter,
    InvalidData,
    ItemNotValidated,
    Unknown(u32),
}

impl CounterStatus {
    /// The number counts only when the status vouches for it. PDH leaves a
    /// value in unsampled slots, and forwarding it would look exactly like a
    /// measurement of zero.
    pub(crate) fn value(self, double_value: f64) -> Option<f64> {
        match self {
            CounterStatus::Valid | CounterStatus::NewData => Some(double_value),
            _ => None,
        }
    }
}

/// Names the status codes PDH documents, so a caller never compares a raw
/// `u32` and never mistakes an unsampled item for an idle one.
pub(crate) fn counter_status(code: u32) -> CounterStatus {
    match code {
        PDH_CSTATUS_VALID_DATA => CounterStatus::Valid,
        PDH_CSTATUS_NEW_DATA => CounterStatus::NewData,
        PDH_CSTATUS_NO_INSTANCE => CounterStatus::NoInstance,
        PDH_CSTATUS_NO_OBJECT => CounterStatus::NoObject,
        PDH_CSTATUS_NO_COUNTER => CounterStatus::NoCounter,
        PDH_CSTATUS_INVALID_DATA => CounterStatus::InvalidData,
        PDH_CSTATUS_ITEM_NOT_VALIDATED => CounterStatus::ItemNotValidated,
        other => CounterStatus::Unknown(other),
    }
}

/// Why a PDH call refused to do the thing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PdhError {
    /// The object is not registered on this machine — a system without a WDDM
    /// driver has no `GPU Engine` at all. This is a fact to remember, not a
    /// failure to repeat on every tick.
    #[error("the performance object is not available on this machine")]
    NoSuchObject,
    #[error("PDH has no data to give yet")]
    NoData,
    #[error("PDH rejected the counter path")]
    InvalidPath,
    /// The instance list outgrew the buffer while it was being copied.
    #[error("the counter array kept growing while it was being read")]
    BufferTooSmall,
    #[error("PDH call failed with status {0:#010x}")]
    Status(u32),
}

/// Codes that mean "this counter has no interval to report on yet". Reading it
/// again on the next tick is the answer; treating it as a failure would hide a
/// working counter behind a transient one.
pub(crate) fn is_no_data_yet(code: u32) -> bool {
    matches!(
        code,
        PDH_NO_DATA | PDH_NO_MORE_DATA | PDH_INVALID_DATA | PDH_CSTATUS_INVALID_DATA
    )
}

pub(crate) fn error_from_status(code: u32) -> PdhError {
    if is_no_data_yet(code) {
        return PdhError::NoData;
    }

    match code {
        PDH_CSTATUS_NO_OBJECT => PdhError::NoSuchObject,
        PDH_INVALID_PATH => PdhError::InvalidPath,
        other => PdhError::Status(other),
    }
}

/// One instance of a wildcard counter and whatever it measured this tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub instance: String,
    pub status: CounterStatus,
    /// `None` unless the status says the counter was actually sampled.
    pub value: Option<f64>,
}

/// Builds a sample from a raw status and the number sitting next to it.
pub(crate) fn sample_of(instance: String, code: u32, double_value: f64) -> Sample {
    let status = counter_status(code);

    Sample {
        instance,
        status,
        value: status.value(double_value),
    }
}

/// A live PDH query. Closing it is dropping it, which also releases every
/// counter added to it.
pub struct Query {
    handle: Owned<PDH_HQUERY>,
}

/// A counter added to a [`Query`]. PDH releases it when the query closes, so it
/// carries the query's lifetime instead of a dangling raw handle.
pub struct Counter<'q> {
    handle: PDH_HCOUNTER,
    query: PhantomData<&'q Query>,
}

impl Query {
    pub fn open() -> Result<Self, PdhError> {
        let mut handle = PDH_HQUERY::default();
        let code = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut handle) };
        if code != PDH_CSTATUS_VALID_DATA {
            return Err(error_from_status(code));
        }

        Ok(Self {
            // Safety: the call above returned a query this handle owns.
            handle: unsafe { Owned::new(handle) },
        })
    }

    /// Adds a counter by its English path so the query reads the same on every
    /// locale. The path is never rebuilt per tick.
    pub fn add(&self, path: &str) -> Result<Counter<'_>, PdhError> {
        let wide = to_wide(path);
        let mut handle = PDH_HCOUNTER::default();
        let code =
            unsafe { PdhAddEnglishCounterW(*self.handle, PCWSTR(wide.as_ptr()), 0, &mut handle) };
        if code != PDH_CSTATUS_VALID_DATA {
            return Err(error_from_status(code));
        }

        Ok(Counter {
            handle,
            query: PhantomData,
        })
    }

    /// Takes one reading for every counter in the query. A rate counter needs
    /// two collections before it has an interval to average over.
    pub fn collect(&self) -> Result<(), PdhError> {
        let code = unsafe { PdhCollectQueryData(*self.handle) };
        // Measured on this machine: the first collection of a rate counter can
        // answer PDH_NO_DATA. The interval advanced anyway, and whether a value
        // exists is a per-item question that `value` and `samples` answer.
        if code == PDH_CSTATUS_VALID_DATA || is_no_data_yet(code) {
            return Ok(());
        }

        Err(error_from_status(code))
    }

    /// The value of a single-instance counter, or `None` when it has not been
    /// sampled yet.
    pub fn value(&self, counter: &Counter<'_>) -> Result<Option<f64>, PdhError> {
        let mut formatted = PDH_FMT_COUNTERVALUE::default();
        let code = unsafe {
            PdhGetFormattedCounterValue(counter.handle, PDH_FMT_DOUBLE, None, &mut formatted)
        };
        // A counter read before its first full interval has no value, which is
        // `None` rather than a failure.
        if is_no_data_yet(code) {
            return Ok(None);
        }

        if code != PDH_CSTATUS_VALID_DATA {
            return Err(error_from_status(code));
        }

        // Safety: the union member matches the requested PDH_FMT_DOUBLE format.
        let double_value = unsafe { formatted.Anonymous.doubleValue };
        Ok(counter_status(formatted.CStatus).value(double_value))
    }

    /// One [`Sample`] per instance of a wildcard counter.
    pub fn samples(&self, counter: &Counter<'_>) -> Result<Vec<Sample>, PdhError> {
        let mut buffer: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::new();
        let mut attempts = 0;

        loop {
            attempts += 1;
            let mut size = (buffer.len() * size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>()) as u32;
            let mut count = 0u32;
            let pointer = if buffer.is_empty() {
                None
            } else {
                Some(buffer.as_mut_ptr())
            };

            // Safety: the buffer, when present, is `size` bytes of the item
            // layout PDH writes.
            let code = unsafe {
                PdhGetFormattedCounterArrayW(
                    counter.handle,
                    PDH_FMT_DOUBLE,
                    &mut size,
                    &mut count,
                    pointer,
                )
            };

            if code == PDH_MORE_DATA {
                if attempts >= MAX_ARRAY_ATTEMPTS {
                    return Err(PdhError::BufferTooSmall);
                }
                // Rounded up: the tail of the buffer holds the instance names
                // that the item pointers refer to.
                let items = (size as usize).div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
                buffer = (0..items)
                    .map(|_| PDH_FMT_COUNTERVALUE_ITEM_W::default())
                    .collect();
                continue;
            }

            if code != PDH_CSTATUS_VALID_DATA {
                return Err(error_from_status(code));
            }

            let items = (count as usize).min(buffer.len());
            let buffer_end = buffer.as_ptr().addr() + size_of_val(buffer.as_slice());
            return Ok(buffer[..items]
                .iter()
                .map(|item| {
                    // The names PDH writes live inside this buffer. A pointer
                    // outside it is a broken answer, not a string to chase, so
                    // it reads as empty.
                    let name = match buffer_end.checked_sub(item.szName.0.addr()) {
                        Some(bytes) => {
                            // Safety: PDH wrote the name inside these bytes.
                            unsafe { text_at(item.szName.0, bytes / size_of::<u16>()) }
                        }
                        None => String::new(),
                    };
                    // Safety: the union member matches the requested format.
                    let double_value = unsafe { item.FmtValue.Anonymous.doubleValue };
                    sample_of(name, item.FmtValue.CStatus, double_value)
                })
                .collect());
        }
    }
}

/// Lists the instances of a performance object.
///
/// PDH caches the object list per process, so a driver that appeared since the
/// last call would stay invisible; the refreshing `PdhEnumObjectsW` call is what
/// makes "is this object here?" a question about now.
pub fn object_instances(object: &str) -> Result<Vec<String>, PdhError> {
    let mut list_length = 0u32;
    let refreshed = unsafe {
        PdhEnumObjectsW(
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            &mut list_length,
            PERF_DETAIL_WIZARD,
            true,
        )
    };
    if refreshed != PDH_CSTATUS_VALID_DATA && refreshed != PDH_MORE_DATA {
        return Err(error_from_status(refreshed));
    }

    let wide = to_wide(object);
    let mut counter_length = 0u32;
    let mut instance_length = 0u32;
    let sizing = unsafe {
        PdhEnumObjectItemsW(
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR(wide.as_ptr()),
            None,
            &mut counter_length,
            None,
            &mut instance_length,
            PERF_DETAIL_WIZARD,
            0,
        )
    };

    if sizing == PDH_MORE_DATA {
        // Both lengths are in characters, terminators included. The counter
        // list is requested because PDH wants a size for it either way; the
        // instances are what the caller is after.
        let mut counters = vec![0u16; counter_length.max(1) as usize];
        let mut instances = vec![0u16; instance_length.max(1) as usize];
        let code = unsafe {
            PdhEnumObjectItemsW(
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR(wide.as_ptr()),
                Some(PWSTR(counters.as_mut_ptr())),
                &mut counter_length,
                Some(PWSTR(instances.as_mut_ptr())),
                &mut instance_length,
                PERF_DETAIL_WIZARD,
                0,
            )
        };
        if code != PDH_CSTATUS_VALID_DATA {
            return Err(error_from_status(code));
        }

        return Ok(multi_text(
            &instances[..instance_length.min(instances.len() as u32) as usize],
        ));
    }

    Err(error_from_status(sizing))
}

/// Reads a NUL-terminated wide string out of a buffer PDH owns.
///
/// `limit` is how many units may be read starting at `start`. Trusting PDH to
/// have terminated a name is not enough: an unterminated one would otherwise
/// be followed for tens of thousands of units past the end of the buffer.
///
/// # Safety
/// `start` must be null, or point at `limit` readable UTF-16 units.
unsafe fn text_at(start: *const u16, limit: usize) -> String {
    if start.is_null() {
        return String::new();
    }

    let limit = limit.min(MAX_WIDE_UNITS);
    let mut units = Vec::with_capacity(limit.min(64));
    for offset in 0..limit {
        // Safety: the caller promised at least `limit` readable units here.
        let unit = unsafe { *start.add(offset) };
        if unit == 0 {
            break;
        }
        units.push(unit);
    }

    String::from_utf16_lossy(&units)
}

/// Splits a double-NUL-terminated list into its entries.
fn multi_text(buffer: &[u16]) -> Vec<String> {
    let mut entries = Vec::new();
    let mut current: Vec<u16> = Vec::new();
    let mut index = 0;

    while index < buffer.len() {
        if buffer[index] == 0 {
            if current.is_empty() {
                break;
            }
            entries.push(String::from_utf16_lossy(&current));
            current.clear();
        } else {
            current.push(buffer[index]);
        }
        index += 1;
    }

    if !current.is_empty() {
        entries.push(String::from_utf16_lossy(&current));
    }

    entries
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use windows::Win32::System::Performance::PDH_INVALID_HANDLE;

    use super::*;

    #[test]
    fn a_counter_status_becomes_a_named_outcome() {
        assert_eq!(counter_status(PDH_CSTATUS_VALID_DATA), CounterStatus::Valid);
        assert_eq!(counter_status(PDH_CSTATUS_NEW_DATA), CounterStatus::NewData);
        assert_eq!(
            counter_status(PDH_CSTATUS_NO_INSTANCE),
            CounterStatus::NoInstance
        );
        assert_eq!(
            counter_status(PDH_CSTATUS_NO_OBJECT),
            CounterStatus::NoObject
        );
        assert_eq!(
            counter_status(PDH_CSTATUS_NO_COUNTER),
            CounterStatus::NoCounter
        );
        assert_eq!(
            counter_status(PDH_CSTATUS_INVALID_DATA),
            CounterStatus::InvalidData
        );
        assert_eq!(
            counter_status(PDH_CSTATUS_ITEM_NOT_VALIDATED),
            CounterStatus::ItemNotValidated
        );
        assert_eq!(
            counter_status(0xDEAD_BEEF),
            CounterStatus::Unknown(0xDEAD_BEEF)
        );
    }

    #[test]
    fn a_call_status_becomes_a_named_error() {
        // An object that is not on this machine is a fact about the machine, not
        // a failure to repeat on every tick.
        assert_eq!(
            error_from_status(PDH_CSTATUS_NO_OBJECT),
            PdhError::NoSuchObject
        );
        assert_eq!(error_from_status(PDH_NO_DATA), PdhError::NoData);
        assert_eq!(
            error_from_status(PDH_INVALID_HANDLE),
            PdhError::Status(PDH_INVALID_HANDLE)
        );
        assert_eq!(
            error_from_status(0xC000_BABE),
            PdhError::Status(0xC000_BABE)
        );
    }

    #[test]
    fn a_call_with_no_sample_yet_is_not_a_failure() {
        // Measured on this machine: a counter read before its first full
        // interval answers with one of these, and the next tick reads a number.
        assert!(is_no_data_yet(PDH_NO_DATA));
        assert!(is_no_data_yet(PDH_NO_MORE_DATA));
        assert!(is_no_data_yet(PDH_INVALID_DATA));
        assert!(is_no_data_yet(PDH_CSTATUS_INVALID_DATA));
        assert!(!is_no_data_yet(PDH_INVALID_HANDLE));
        assert!(!is_no_data_yet(PDH_CSTATUS_NO_OBJECT));

        assert_eq!(
            error_from_status(PDH_CSTATUS_INVALID_DATA),
            PdhError::NoData
        );
        assert_eq!(error_from_status(PDH_INVALID_DATA), PdhError::NoData);
    }

    #[test]
    fn only_a_sampled_counter_carries_a_value() {
        let sampled = sample_of("pid_1".to_string(), PDH_CSTATUS_VALID_DATA, 12.5);
        assert_eq!(sampled.value, Some(12.5));
        assert_eq!(sampled.status, CounterStatus::Valid);

        // The number PDH leaves in an unsampled slot is not a measurement, and
        // reporting it would look identical to an idle engine.
        let unsampled = sample_of("pid_1".to_string(), PDH_CSTATUS_NO_INSTANCE, 99.0);
        assert_eq!(unsampled.value, None);
        assert_eq!(unsampled.status, CounterStatus::NoInstance);

        let invalid = sample_of("pid_1".to_string(), PDH_CSTATUS_INVALID_DATA, 7.0);
        assert_eq!(invalid.value, None);
    }

    #[test]
    fn a_wide_string_is_read_up_to_its_nul() {
        let buffer: Vec<u16> = "notepad.exe".encode_utf16().chain([0, 0, 0]).collect();
        assert_eq!(
            unsafe { text_at(buffer.as_ptr(), buffer.len()) },
            "notepad.exe"
        );

        let empty = [0u16];
        assert_eq!(unsafe { text_at(empty.as_ptr(), empty.len()) }, "");

        let null = std::ptr::null();
        assert_eq!(unsafe { text_at(null, 0) }, "");
    }

    #[test]
    fn a_name_without_a_terminator_stops_at_the_buffer() {
        // No NUL anywhere: the limit, not the search, is what ends the read.
        let buffer: Vec<u16> = "chrome".encode_utf16().collect();

        assert_eq!(unsafe { text_at(buffer.as_ptr(), buffer.len()) }, "chrome");
        assert_eq!(unsafe { text_at(buffer.as_ptr(), 3) }, "chr");
    }

    #[test]
    fn a_double_terminated_list_splits_into_entries() {
        let buffer: Vec<u16> = "Utilization Percentage"
            .encode_utf16()
            .chain([0])
            .chain("Running Time".encode_utf16())
            .chain([0, 0])
            .collect();

        assert_eq!(
            multi_text(&buffer),
            vec![
                "Utilization Percentage".to_string(),
                "Running Time".to_string()
            ]
        );
        assert!(multi_text(&[0, 0]).is_empty());
        assert!(multi_text(&[]).is_empty());
    }

    // The tests above name the statuses. These prove the names match what the
    // local PDH service actually returns.

    #[test]
    fn a_total_counter_reads_a_number_after_two_collects() {
        let query = Query::open().expect("pdh query");
        let counter = query
            .add(r"\Process(_Total)\% Processor Time")
            .expect("english counter path");

        // The first collect only opens the interval; a rate counter has no
        // value until a second one closes it.
        query.collect().expect("first collect");
        // Windows recomputes performance data once per second, so two collects
        // closer than that leave PDH with the same snapshot twice.
        std::thread::sleep(std::time::Duration::from_millis(1_500));
        query.collect().expect("second collect");

        // The first read of a freshly added counter is the call PDH uses to
        // build its state, and it answers with nothing. Reading again after the
        // next collect is what a running sampler does.
        let _first_read = query.value(&counter);
        query.collect().expect("third collect");

        let value = query.value(&counter).expect("formatted value");
        assert!(
            value.is_some_and(|percent| percent.is_finite()),
            "the _Total processor counter must read a finite percentage, got {value:?}"
        );
    }
    #[test]
    fn a_wildcard_counter_returns_one_sample_per_instance() {
        let query = Query::open().expect("pdh query");
        let counter = query
            .add(r"\Process(*)\% Processor Time")
            .expect("wildcard counter path");
        query.collect().expect("first collect");
        // Windows recomputes performance data once per second, so two collects
        // closer than that leave PDH with the same snapshot twice.
        std::thread::sleep(std::time::Duration::from_millis(1_500));
        query.collect().expect("second collect");

        // Measured on this machine: the first array read of a fresh wildcard
        // counter answers "no data yet" while PDH builds the instance list. The
        // next tick reads the values, which is the cadence the scheduler uses.
        let _first_read = query.samples(&counter);
        query.collect().expect("third collect");

        let samples = query.samples(&counter).expect("counter array");
        assert!(!samples.is_empty(), "at least one process instance");
        assert!(
            samples.iter().all(|sample| !sample.instance.is_empty()),
            "every item in the array carries the name PDH read from it"
        );
        assert!(
            samples.iter().any(|sample| sample.value.is_some()),
            "a machine running processes must report at least one sampled instance"
        );
    }

    #[test]
    fn an_unknown_object_is_reported_as_absent() {
        let outcome = object_instances("Easy Window Not A Performance Object");

        assert_eq!(outcome, Err(PdhError::NoSuchObject));
    }

    #[test]
    fn the_process_object_lists_instances() {
        let instances = object_instances("Process").expect("process object");

        assert!(
            instances.iter().any(|instance| instance == "System"),
            "the System process is always an instance of the Process object"
        );
    }

    #[test]
    fn the_gpu_object_is_listed_or_reported_absent() {
        // A machine without a WDDM driver has no `GPU Engine` object at all.
        // Either answer is a result; anything else is the wrapper failing.
        let outcome = object_instances("GPU Engine");

        assert!(
            matches!(outcome, Ok(_) | Err(PdhError::NoSuchObject)),
            "unexpected outcome for the GPU Engine object: {outcome:?}"
        );
    }
}
