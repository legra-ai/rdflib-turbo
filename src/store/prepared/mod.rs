//! Typed snapshots of prepared RDFLib algebra. No Python objects cross a worker boundary.
mod blank_nodes;
mod census;
mod expression;
mod pattern;
mod snapshot;
mod term;

pub(super) use snapshot::prepare;
