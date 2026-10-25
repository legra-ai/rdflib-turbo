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
    clause_ordinal: Option<usize>,
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
        clause_ordinal: None,
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
    emit_snapshot(writer, &snapshot(event, round, heads))
}

fn emit_clause_into(
    writer: &mut impl Write,
    event: &'static str,
    round: usize,
    ordinal: usize,
    heads: (usize, usize),
) -> io::Result<()> {
    if !ENABLED.load(Relaxed) {
        return Ok(());
    }
    let mut record = snapshot(event, round, heads);
    record.clause_ordinal = Some(ordinal);
    emit_snapshot(writer, &record)
}

fn emit_snapshot(writer: &mut impl Write, record: &Snapshot<'_>) -> io::Result<()> {
    assert!(
        record.event.len() <= 48,
        "allocation trace label exceeds its fixed bound"
    );
    // bounded: nine scalar/static fields, <=48-byte label and <=20 decimal digits/usize.
    let mut bytes = [0; 512];
    let mut output = Cursor::new(bytes.as_mut_slice());
    serde_json::to_writer(&mut output, record).map_err(io::Error::other)?;
    let size = output.position() as usize;
    writer.write_all(b"MEMORY_TRACE ")?;
    writer.write_all(&bytes[..size])?;
    writer.write_all(b"\n")
}

pub(crate) fn mark_clause(
    event: &'static str,
    round: usize,
    ordinal: usize,
    heads: (usize, usize),
) {
    if !ENABLED.load(Relaxed) {
        return;
    }
    emit_clause_into(&mut io::stdout().lock(), event, round, ordinal, heads)
        .expect("write indexed allocation trace");
}

pub(crate) fn mark(event: &'static str, round: usize, heads: (usize, usize)) {
    if !ENABLED.load(Relaxed) {
        return;
    }
    emit_into(&mut io::stdout().lock(), event, round, heads).expect("write allocation trace");
}

pub(crate) fn pause() {
    ENABLED.store(false, Relaxed);
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
        emit_clause_into(&mut output, "inactive-clause", 4, 12, (5, 6)).unwrap();
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

    #[test]
    #[ignore = "exact isolated allocator process required"]
    fn paused_trace_does_not_observe_the_independent_semantic_replay() {
        let _guard = Guard::enable();
        pause();
        let mut bytes = [0; 512];
        let mut output = Cursor::new(bytes.as_mut_slice());
        let before = counters();
        emit_into(&mut output, "semantic-replay", 0, (0, 0)).unwrap();
        mark("semantic-replay", 0, (0, 0));
        assert_eq!(counters(), before);
        assert_eq!(output.position(), 0);
    }
    #[test]
    #[ignore = "exact isolated allocator process required"]
    fn indexed_trace_retains_actual_round_and_catalogue_ordinal_without_allocating() {
        let _guard = Guard::enable();
        let mut bytes = [0; 512];
        let mut output = Cursor::new(bytes.as_mut_slice());
        let before = counters();
        emit_clause_into(&mut output, "clause_before", 7, 12, (5, 6)).unwrap();
        assert_eq!(counters(), before);
        let size = output.position() as usize;
        let text = std::str::from_utf8(&bytes[..size]).unwrap();
        let record: Snapshot<'_> =
            serde_json::from_str(text.strip_prefix("MEMORY_TRACE ").unwrap().trim_end()).unwrap();
        assert_eq!((record.round, record.clause_ordinal), (7, Some(12)));
        assert_eq!((record.owned_heads, record.native_heads), (5, 6));
    }

    #[test]
    #[ignore = "exact isolated allocator process required"]
    fn largest_scalar_snapshot_fits_the_fixed_buffer_without_allocating() {
        let _guard = Guard::enable();
        let expected = Snapshot {
            event: "123456789012345678901234567890123456789012345678",
            round: usize::MAX,
            clause_ordinal: Some(usize::MAX),
            owned_heads: usize::MAX,
            native_heads: usize::MAX,
            live_bytes: usize::MAX,
            peak_bytes: usize::MAX,
            allocated_bytes: usize::MAX,
            allocation_calls: usize::MAX,
        };
        let mut bytes = [0; 512];
        let mut output = Cursor::new(bytes.as_mut_slice());
        let before = counters();
        emit_snapshot(&mut output, &expected).unwrap();
        assert_eq!(counters(), before);
        let size = output.position() as usize;
        assert!(size > 0, "maximum-width trace emitted no snapshot");
        let text = std::str::from_utf8(&bytes[..size]).unwrap();
        let record: Snapshot<'_> =
            serde_json::from_str(text.strip_prefix("MEMORY_TRACE ").unwrap().trim_end()).unwrap();
        assert_eq!(record, expected);
    }
}
