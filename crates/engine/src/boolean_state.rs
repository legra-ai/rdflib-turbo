//! Checked builtin-Boolean presentation, separate from RDF term identity.
use crate::{Error, Result};

/// Exact builtin cache and tri-state assessment captured from an original Literal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum BuiltinBooleanState {
    /// False cache, no assessment.
    FalseUnassessed = 1,
    /// False cache, valid assessment.
    FalseValid = 2,
    /// False cache, invalid assessment.
    FalseInvalid = 3,
    /// True cache, no assessment.
    TrueUnassessed = 4,
    /// True cache, valid assessment.
    TrueValid = 5,
    /// True cache, invalid assessment.
    TrueInvalid = 6,
}
impl BuiltinBooleanState {
    /// Capture already checked builtin payloads without conversion or truth coercion.
    pub const fn new(value: bool, assessment: Option<bool>) -> Self {
        match (value, assessment) {
            (false, None) => Self::FalseUnassessed,
            (false, Some(false)) => Self::FalseValid,
            (false, Some(true)) => Self::FalseInvalid,
            (true, None) => Self::TrueUnassessed,
            (true, Some(false)) => Self::TrueValid,
            (true, Some(true)) => Self::TrueInvalid,
        }
    }
    /// Decode only the closed typed descriptor domain.
    pub fn from_code(code: u8) -> Result<Self> {
        Ok(match code {
            1 => Self::FalseUnassessed,
            2 => Self::FalseValid,
            3 => Self::FalseInvalid,
            4 => Self::TrueUnassessed,
            5 => Self::TrueValid,
            6 => Self::TrueInvalid,
            _ => return Err(Error::Invalid("Invalid builtin Boolean state".into())),
        })
    }
    /// Typed transfer representation; not an RDF datatype or lexical marker.
    pub const fn code(self) -> u8 {
        self as u8
    }
    /// Actual original builtin cache.
    pub const fn value(self) -> bool {
        matches!(
            self,
            Self::TrueUnassessed | Self::TrueValid | Self::TrueInvalid
        )
    }
    /// Original assessment, preserving None versus False.
    pub const fn assessment(self) -> Option<bool> {
        match self {
            Self::FalseUnassessed | Self::TrueUnassessed => None,
            Self::FalseValid | Self::TrueValid => Some(false),
            Self::FalseInvalid | Self::TrueInvalid => Some(true),
        }
    }
    /// Canonical lexical conversion's ordinary assessment, only after source completeness.
    pub fn lexical_default(word: &str) -> Option<Self> {
        match word {
            "false" => Some(Self::FalseValid),
            "true" => Some(Self::TrueValid),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_descriptor_preserves_all_six_states_and_rejects_unknown_codes() {
        for value in [false, true] {
            for assessment in [None, Some(false), Some(true)] {
                let state = BuiltinBooleanState::new(value, assessment);
                assert_eq!(state.value(), value);
                assert_eq!(state.assessment(), assessment);
                assert_eq!(BuiltinBooleanState::from_code(state.code()).unwrap(), state);
            }
        }
        for code in [0, 7, 255] {
            assert!(BuiltinBooleanState::from_code(code).is_err());
        }
        assert_eq!(std::mem::size_of::<BuiltinBooleanState>(), 1);
        assert_eq!(std::mem::size_of::<Option<BuiltinBooleanState>>(), 1);
    }
}

#[cfg(test)]
mod consumer_tests {
    use super::*;
    use crate::oxigraph::model::*;
    use crate::{CompiledShapes, GraphRead, Options};
    struct Observed<'a> {
        data: &'a Dataset,
        state: BuiltinBooleanState,
    }
    impl<'source> GraphRead for Observed<'source> {
        fn selected_focus_source(&self) -> Option<&dyn crate::SelectedFocusSource> {
            None
        }
        fn selected_value_source(&self) -> Option<&dyn crate::SelectedValueSource> {
            None
        }

        fn boolean_object_state(&self, _: &GeneralizedQuad) -> Result<Option<BuiltinBooleanState>> {
            Ok(Some(self.state))
        }
        fn matching_quads<'a>(
            &'a self,
            s: Option<TermRef<'_>>,
            p: Option<TermRef<'_>>,
            o: Option<TermRef<'_>>,
            g: Option<GraphNameRef<'_>>,
        ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'source> {
            self.data.matching_quads(s, p, o, g)
        }
        fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
            self.data.named_graphs()
        }
        fn contains_named_graph(&self, g: &NamedOrBlankNode) -> Result<bool> {
            GraphRead::contains_named_graph(self.data, g)
        }
    }
    #[test]
    fn actual_datatype_consumer_rejects_invalid_canonical_boolean_and_retains_report_slot() {
        let shapes = r#"@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetNode <urn:s>; sh:path <urn:p>; sh:datatype <http://www.w3.org/2001/XMLSchema#boolean>."#;
        let compiled = CompiledShapes::compile(
            crate::IndexedShapesSource::parse_graph(
                shapes,
                crate::oxigraph::io::RdfFormat::Turtle,
                None,
            )
            .unwrap(),
            false,
        )
        .unwrap();
        let mut data = Dataset::new();
        data.insert(&Quad::new(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::from(false),
            GraphName::DefaultGraph,
        ));
        for state in [
            BuiltinBooleanState::FalseInvalid,
            BuiltinBooleanState::FalseValid,
            BuiltinBooleanState::FalseUnassessed,
        ] {
            let result = compiled
                .validate_readonly(&Observed { data: &data, state }, Options::default())
                .unwrap();
            assert_eq!(result.conforms, state != BuiltinBooleanState::FalseInvalid);
            if state == BuiltinBooleanState::FalseInvalid {
                assert_eq!(result.graph.len(), 11);
                assert_eq!(result.boolean_values.len(), 1);
                assert!(
                    matches!(&result.boolean_values[0].2, crate::BooleanObservation::Attested(actual) if *actual == state)
                );
            } else {
                assert_eq!(result.graph.len(), 2);
                assert!(result.boolean_values.is_empty());
            }
        }
    }
}
