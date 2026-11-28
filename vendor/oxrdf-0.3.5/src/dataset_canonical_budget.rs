//! Caller-set work limits for RDFC-1.0 blank-node labelling (Turbo patch).
//!
//! The RDFC-1.0 specification lets implementations abort on "poison"
//! datasets whose Hash N-Degree Quads search grows factorially. One work unit
//! is one Hash N-Degree Quads invocation or one permutation of a related
//! blank-node list. Permutations are charged before they are generated, so an
//! exhausted budget never materializes a factorial list.
use std::cell::Cell;
use std::fmt;
use std::num::NonZeroU64;

/// The maximum number of RDFC-1.0 work units one canonicalization may spend.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct CanonicalizationBudget {
    units: Option<NonZeroU64>,
}

impl CanonicalizationBudget {
    /// A budget of at most `units` work units.
    pub const fn limited(units: NonZeroU64) -> Self {
        Self { units: Some(units) }
    }

    /// No limit: the upstream behaviour, which may run for exponential time.
    pub const fn unlimited() -> Self {
        Self { units: None }
    }

    /// The configured limit, or `None` when unlimited.
    pub const fn units(self) -> Option<NonZeroU64> {
        self.units
    }

    pub(super) fn meter(self) -> WorkMeter {
        WorkMeter {
            limit: self.units,
            spent: Cell::new(0),
        }
    }
}

/// The RDFC-1.0 work budget was exhausted before every blank node was labelled.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CanonicalizationBudgetExceeded {
    limit: NonZeroU64,
}

impl CanonicalizationBudgetExceeded {
    /// The work budget that was exhausted.
    pub const fn limit(self) -> NonZeroU64 {
        self.limit
    }
}

impl fmt::Display for CanonicalizationBudgetExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RDFC-1.0 canonicalization exceeded its work budget of {} units",
            self.limit
        )
    }
}

impl std::error::Error for CanonicalizationBudgetExceeded {}

/// Counts spent work units during one canonicalization.
pub(super) struct WorkMeter {
    limit: Option<NonZeroU64>,
    spent: Cell<u64>,
}

impl WorkMeter {
    /// Spends one unit, failing once the limit has been reached.
    pub(super) fn spend(&self) -> Result<(), CanonicalizationBudgetExceeded> {
        self.charge(1)
    }

    /// Spends one unit per permutation of `items` blank nodes (`items!`).
    pub(super) fn spend_permutations(
        &self,
        items: usize,
    ) -> Result<(), CanonicalizationBudgetExceeded> {
        let permutations = (2..=items as u64).try_fold(1_u64, u64::checked_mul);
        self.charge(permutations.unwrap_or(u64::MAX))
    }

    fn charge(&self, units: u64) -> Result<(), CanonicalizationBudgetExceeded> {
        let spent = self.spent.get().saturating_add(units);
        self.spent.set(spent);
        match self.limit {
            Some(limit) if spent > limit.get() => Err(CanonicalizationBudgetExceeded { limit }),
            _ => Ok(()),
        }
    }
}
