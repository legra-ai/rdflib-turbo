use pyo3::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rounding {
    Ceiling,
    Down,
    Floor,
    HalfDown,
    HalfEven,
    HalfUp,
    Up,
    ZeroFiveUp,
}

#[derive(Debug, Clone)]
pub(super) struct DecimalContext {
    pub precision: usize,
    pub rounding: Rounding,
    pub emin: i64,
    pub emax: i64,
    pub clamp: bool,
    // bounded: seven fixed Decimal signals, never graph-derived state.
    pub traps: [bool; 7],
}

pub(super) const INVALID: usize = 0;
pub(super) const INEXACT: usize = 1;
pub(super) const ROUNDED: usize = 2;
pub(super) const SUBNORMAL: usize = 3;
pub(super) const UNDERFLOW: usize = 4;
pub(super) const OVERFLOW: usize = 5;
pub(super) const CLAMPED: usize = 6;

#[derive(Debug, Clone)]
pub(crate) struct LexicalContext {
    pub integer_digit_limit: usize,
    pub(super) decimal: DecimalContext,
    pub(super) hour_leading_space: bool,
}

impl Default for LexicalContext {
    fn default() -> Self {
        Self {
            integer_digit_limit: 4300,
            decimal: DecimalContext {
                precision: 28,
                rounding: Rounding::HalfEven,
                emin: -999999,
                emax: 999999,
                clamp: false,
                traps: [true, false, false, false, false, true, false],
            },
            hour_leading_space: true,
        }
    }
}

impl LexicalContext {
    /// Snapshot the calling Python thread before moving native inference work.
    pub(crate) fn capture(py: Python<'_>) -> PyResult<Self> {
        let decimal = py.import("decimal")?;
        let context = decimal.call_method0("getcontext")?;
        let traps = context.getattr("traps")?;
        let names = [
            "InvalidOperation",
            "Inexact",
            "Rounded",
            "Subnormal",
            "Underflow",
            "Overflow",
            "Clamped",
        ];
        let mut flags = [false; 7];
        for (index, name) in names.into_iter().enumerate() {
            flags[index] = traps.get_item(decimal.getattr(name)?)?.extract()?;
        }
        let rounding: String = context.getattr("rounding")?.extract()?;
        let rounding = match rounding.as_str() {
            "ROUND_CEILING" => Rounding::Ceiling,
            "ROUND_DOWN" => Rounding::Down,
            "ROUND_FLOOR" => Rounding::Floor,
            "ROUND_HALF_DOWN" => Rounding::HalfDown,
            "ROUND_HALF_EVEN" => Rounding::HalfEven,
            "ROUND_HALF_UP" => Rounding::HalfUp,
            "ROUND_UP" => Rounding::Up,
            "ROUND_05UP" => Rounding::ZeroFiveUp,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Unknown Decimal rounding mode",
                ));
            }
        };
        let strptime = py.import("_strptime")?.getattr("_TimeRE_cache")?;
        let hour: String = strptime.get_item("H")?.extract()?;
        Ok(Self {
            integer_digit_limit: py
                .import("sys")?
                .call_method0("get_int_max_str_digits")?
                .extract()?,
            decimal: DecimalContext {
                precision: context.getattr("prec")?.extract()?,
                rounding,
                emin: context.getattr("Emin")?.extract()?,
                emax: context.getattr("Emax")?.extract()?,
                clamp: context.getattr("clamp")?.extract::<u8>()? != 0,
                traps: flags,
            },
            hour_leading_space: hour.contains(' '),
        })
    }
}
