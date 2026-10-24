//! The shared structural model instantiated with this runtime's exact RDF terms.
//!
//! Pattern bodies, counts and flags retain their original RDF terms until lowering.
//! Named roles preserve RDFLib URIRef lexical identity as native NamedNode;
//! the shared model's default strict RFC IRI domain remains unchanged.

use oxigraph::model::{NamedNode, Term};

pub(crate) use shacl_structural_model::*;

pub(crate) type Component<T = Term> = shacl_structural_model::Component<T, T, T, T, NamedNode>;
pub(crate) type NodeShape<T = Term> = shacl_structural_model::NodeShape<T, T, T, T, NamedNode>;
pub(crate) type PropertyShape<T = Term> =
    shacl_structural_model::PropertyShape<T, T, T, T, NamedNode>;
pub(crate) type Parameter<T = Term> = shacl_structural_model::Parameter<T, T, T, T, NamedNode>;
pub(crate) type ShaclFunction<T = Term> =
    shacl_structural_model::ShaclFunction<T, T, T, T, NamedNode>;
pub(crate) type SparqlTargetType<T = Term> =
    shacl_structural_model::SparqlTargetType<T, T, T, T, NamedNode>;
pub(crate) type ShaclPath = shacl_structural_model::ShaclPath<NamedNode>;
pub(crate) type Target<T = Term> = shacl_structural_model::Target<T, NamedNode>;
pub(crate) type CustomTarget<T = Term> = shacl_structural_model::CustomTarget<T, NamedNode>;
pub(crate) type ParameterValues<T = Term> = shacl_structural_model::ParameterValues<T, NamedNode>;
pub(crate) type CustomConstraint<T = Term> = shacl_structural_model::CustomConstraint<T, NamedNode>;
pub(crate) type NodeExpression<T = Term> = shacl_structural_model::NodeExpression<T, NamedNode>;
pub(crate) type ExpressionKind<T = Term> = shacl_structural_model::ExpressionKind<T, NamedNode>;
pub(crate) type ExpressionConstraint<T = Term> =
    shacl_structural_model::ExpressionConstraint<T, NamedNode>;
pub(crate) type ShaclRule<T = Term> = shacl_structural_model::ShaclRule<T, NamedNode>;
pub(crate) type RuleKind<T = Term> = shacl_structural_model::RuleKind<T, NamedNode>;
pub(crate) type TripleRule<T = Term> = shacl_structural_model::TripleRule<T, NamedNode>;
