//! Source requests attach on their original caller; all other work stays detached.
use super::{
    ComparisonInvocation, NativeValueComparisonPolicy, PyCompiledShapes, ReportOutput,
    SourceComparisonInvocation, boolean_input, terms,
};
use pyo3::prelude::*;
use pyshacl_turbo_engine::oxigraph::model::FrozenDataset;
use pyshacl_turbo_engine::{CoreComparisonPolicy, Options, Result};
use std::sync::Arc;

pub(super) enum Comparison {
    Lexical(ComparisonInvocation),
    Selected(SourceComparisonInvocation),
}
impl Comparison {
    pub fn capture(
        py: Python<'_>,
        policy: Py<NativeValueComparisonPolicy>,
        candidates: bool,
    ) -> PyResult<Self> {
        if candidates {
            Ok(Self::Selected(
                super::comparison_host(py, policy)?.capture_source(py)?,
            ))
        } else {
            Ok(Self::Lexical(super::capture_comparison(py, policy)?))
        }
    }
    pub fn authority(&self) -> CoreComparisonPolicy {
        match self {
            Self::Lexical(c) => c.authority(),
            Self::Selected(c) => CoreComparisonPolicy::Host(c.policy.clone()),
        }
    }
}
impl PyCompiledShapes {
    pub(super) fn validate_boolean(
        &self,
        py: Python<'_>,
        data: Arc<FrozenDataset>,
        source: Py<boolean_input::BooleanInput>,
        options: Result<Options>,
        include_data: bool,
        comparison: Comparison,
    ) -> PyResult<ReportOutput> {
        enum Finish {
            Lexical(ComparisonInvocation),
            Selected(Arc<boolean_input::caller::Caller>),
        }
        let (caller, finish) = match comparison {
            Comparison::Lexical(c) => (None, Finish::Lexical(c)),
            Comparison::Selected(c) => {
                let caller = boolean_input::caller::Caller::new(py, source.bind(py).get(), c)?;
                (Some(caller.clone()), Finish::Selected(caller))
            }
        };
        let result = (|| {
            let mut options = options?;
            if let Some(caller) = &caller {
                options.comparison = caller.authority();
            }
            let captured = boolean_input::selection::Captured {
                data: &data,
                caller,
            };
            py.detach(|| {
                let outcome = self.inner.validate_delta(&captured, options)?;
                let report = outcome.report;
                Ok((
                    report.conforms,
                    terms::Output::observed_report(&report.graph, true, &report.boolean_values)?,
                    report.text,
                    include_data.then(|| terms::Output::expanded(&data, &outcome.additions)),
                    report.recursion_warnings,
                ))
            })
        })();
        let (conforms, report, text, expanded, warnings) = match finish {
            Finish::Lexical(c) => c.finish(py, result)?,
            Finish::Selected(c) => c.finish(py, result)?,
        };
        Ok((
            conforms,
            report.into_python(py)?,
            text,
            expanded.map(|o| o.into_python(py)).transpose()?,
            warnings,
        ))
    }
}
