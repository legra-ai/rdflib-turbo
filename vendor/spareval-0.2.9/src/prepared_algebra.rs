//! An owned prepared RDFLib plan is bound to its exact source algebra.
use spargebra::Query;
use spargebra::algebra::GraphPattern as SourcePattern;
use sparopt::algebra::GraphPattern;
use std::sync::Arc;

/// A typed native plan preserving supplied RDFLib join and OPTIONAL metadata.
/// No query source is serialized or reparsed. The source tree is derived from
/// this plan and checked against the query before execution.
#[derive(Clone)]
pub struct RdfLibPreparedAlgebra {
    pub(crate) source: SourcePattern,
    pub(crate) pattern: Arc<GraphPattern>,
}

impl RdfLibPreparedAlgebra {
    /// Own a prepared plan and its structurally matching source algebra.
    pub fn new(pattern: GraphPattern) -> Self {
        Self {
            source: SourcePattern::from(&pattern),
            pattern: Arc::new(pattern),
        }
    }

    /// The actual parsed query pattern for use with the query's form and prologue.
    pub fn source(&self) -> &SourcePattern {
        &self.source
    }

    pub(crate) fn matches(&self, query: &Query) -> bool {
        let (Query::Select { pattern, .. }
        | Query::Ask { pattern, .. }
        | Query::Construct { pattern, .. }
        | Query::Describe { pattern, .. }) = query;
        pattern == &self.source
    }
}
