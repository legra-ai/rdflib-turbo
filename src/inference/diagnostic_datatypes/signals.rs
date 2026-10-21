//! Invocation-owned status signals; Python publication happens after native work.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

use pyo3::prelude::*;

// Includes the complete Decimal Context.flags vocabulary, even signals that the
// present string conversion profile cannot raise (DivisionByZero/FloatOperation).
#[derive(Clone, Copy)]
#[repr(u8)]
enum Signal {
    InvalidOperation,
    Inexact,
    Rounded,
    Subnormal,
    Underflow,
    Overflow,
    Clamped,
    DivisionByZero,
    FloatOperation,
}

impl Signal {
    const ALL: [Self; 9] = [
        Self::InvalidOperation,
        Self::Inexact,
        Self::Rounded,
        Self::Subnormal,
        Self::Underflow,
        Self::Overflow,
        Self::Clamped,
        Self::DivisionByZero,
        Self::FloatOperation,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::InvalidOperation => "InvalidOperation",
            Self::Inexact => "Inexact",
            Self::Rounded => "Rounded",
            Self::Subnormal => "Subnormal",
            Self::Underflow => "Underflow",
            Self::Overflow => "Overflow",
            Self::Clamped => "Clamped",
            Self::DivisionByZero => "DivisionByZero",
            Self::FloatOperation => "FloatOperation",
        }
    }

    fn bit(self) -> u16 {
        1 << self as u8
    }
}

#[derive(Debug, Default)]
pub(super) struct Signals(AtomicU16);

impl Signals {
    pub(super) fn record(&self, signals: [bool; 7]) {
        let mut bits = 0;
        for (signal, raised) in Signal::ALL.into_iter().zip(signals) {
            if raised {
                bits |= signal.bit();
            }
        }
        if bits != 0 {
            self.0.fetch_or(bits, Ordering::Relaxed);
        }
    }
}

/// Lives exclusively on the calling Python thread, never in a native request.
pub(crate) struct SignalPublisher {
    // bounded: one caller mapping and the nine fixed Decimal signal classes.
    flags: Py<PyAny>,
    classes: [Py<PyAny>; 9],
    signals: Arc<Signals>,
}

impl SignalPublisher {
    pub(super) fn new(
        decimal: &Bound<'_, PyAny>,
        context: &Bound<'_, PyAny>,
        signals: Arc<Signals>,
    ) -> PyResult<Self> {
        let mut classes = Vec::with_capacity(9); // bounded: fixed signal vocabulary.
        for signal in Signal::ALL {
            classes.push(decimal.getattr(signal.name())?.unbind());
        }
        Ok(Self {
            flags: context.getattr("flags")?.unbind(),
            classes: classes.try_into().expect("nine Decimal signals"),
            signals,
        })
    }

    /// The caller must finish/join all native producers before entering here.
    pub(crate) fn complete<T>(self, py: Python<'_>, result: crate::Result<T>) -> PyResult<T> {
        let bits = self.signals.0.load(Ordering::Relaxed);
        for (signal, class) in Signal::ALL.into_iter().zip(&self.classes) {
            if bits & signal.bit() != 0 {
                self.flags.bind(py).set_item(class, true)?;
            }
        }
        result.map_err(crate::error)
    }
}
