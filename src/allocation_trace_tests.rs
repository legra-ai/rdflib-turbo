//! Isolated source-test tracing; no production observer, state or counter reset.
use std::io::{self, Cursor, Write};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use serde::{Deserialize, Serialize};

use super::{CALLS, LIVE, PEAK, TOTAL};

static ENABLED: AtomicBool = AtomicBool::new(false);

pub(crate) struct Guard {
    _private: (),
}

impl Guard {
    pub(crate) fn enable() -> Self {
        ENABLED
            .compare_exchange(false, true, Relaxed, Relaxed)
            .expect("allocation trace scope already active");
        Self { _private: () }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        ENABLED.store(false, Relaxed);
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Snapshot<'a> {
    event: &'a str,
    round: usize,
    owned_heads: usize,
    native_heads: usize,
    live_bytes: usize,
    peak_bytes: usize,
    allocated_bytes: usize,
    allocation_calls: usize,
}

fn snapshot(event: &'static str, round: usize, heads: (usize, usize)) -> Snapshot<'static> {
    Snapshot {
        event,
        round,
        owned_heads: heads.0,
        native_heads: heads.1,
        live_bytes: LIVE.load(Relaxed),
        peak_bytes: PEAK.load(Relaxed),
        allocated_bytes: TOTAL.load(Relaxed),
        allocation_calls: CALLS.load(Relaxed),
    }
}

fn emit_into(
    writer: &mut impl Write,
    event: &'static str,
    round: usize,
    heads: (usize, usize),
) -> io::Result<()> {
    if !ENABLED.load(Relaxed) {
        return Ok(());
    }
    assert!(
        event.len() <= 48,
        "allocation trace label exceeds its fixed bound"
    );
    let record = snapshot(event, round, heads);
    // bounded: nine scalar/static fields, a <=48-byte label and <=20 decimal digits/usize.
    let mut bytes = [0; 512];
    let mut output = Cursor::new(bytes.as_mut_slice());
    serde_json::to_writer(&mut output, &record).map_err(io::Error::other)?;
    let size = output.position() as usize;
    writer.write_all(b"MEMORY_TRACE ")?;
    writer.write_all(&bytes[..size])?;
    writer.write_all(b"\n")
}

pub(crate) fn mark(event: &'static str, round: usize, heads: (usize, usize)) {
    if !ENABLED.load(Relaxed) {
        return;
    }
    emit_into(&mut io::stdout().lock(), event, round, heads).expect("write allocation trace");
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;

    fn counters() -> [usize; 4] {
        [
            LIVE.load(Relaxed),
            PEAK.load(Relaxed),
            TOTAL.load(Relaxed),
            CALLS.load(Relaxed),
        ]
    }

    #[test]
    #[ignore = "exact isolated allocator process required"]
    fn inactive_trace_allocates_nothing_and_preserves_all_counters() {
        assert!(!ENABLED.load(Relaxed));
        let mut bytes = [0; 512];
        let mut output = Cursor::new(bytes.as_mut_slice());
        let before = counters();
        emit_into(&mut output, "inactive", 4, (5, 6)).unwrap();
        mark("inactive-stdout", 4, (5, 6));
        let after = counters();
        assert_eq!(before, after);
        assert_eq!(output.position(), 0);
    }

    #[test]
    #[ignore = "exact isolated allocator process required"]
    fn active_trace_emits_the_unchanged_snapshot_without_allocating() {
        let _guard = Guard::enable();
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"MEMORY_TRACE_WARMUP ").unwrap();
        stdout.write_all(b"ready\n").unwrap();
        stdout.flush().unwrap();
        drop(stdout);
        let mut bytes = [0; 512];
        let mut output = Cursor::new(bytes.as_mut_slice());
        let before = counters();
        emit_into(&mut output, "active-probe", 4, (5, 6)).unwrap();
        let after = counters();
        assert_eq!(before, after);
        let size = output.position() as usize;
        assert!(size > 0, "active trace emitted no allocator snapshot");
        let record = std::str::from_utf8(&bytes[..size]).unwrap();
        let record = record.strip_prefix("MEMORY_TRACE ").unwrap().trim_end();
        let record: Snapshot<'_> = serde_json::from_str(record).unwrap();
        assert_eq!(record.event, "active-probe");
        assert_eq!(
            (record.round, record.owned_heads, record.native_heads),
            (4, 5, 6)
        );
        assert_eq!(
            [
                record.live_bytes,
                record.peak_bytes,
                record.allocated_bytes,
                record.allocation_calls
            ],
            before
        );
        let before_stdout = counters();
        mark("stdout-probe", 4, (5, 6));
        assert_eq!(counters(), before_stdout);
    }

    #[test]
    #[ignore = "exact isolated trace scope process required"]
    fn trace_guard_restores_the_disabled_state() {
        assert!(!ENABLED.load(Relaxed));
        {
            let _guard = Guard::enable();
            assert!(ENABLED.load(Relaxed));
        }
        assert!(!ENABLED.load(Relaxed));
    }
}
