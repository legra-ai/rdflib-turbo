#![cfg_attr(doc, doc = include_str!("../README.md"))]
#![doc(test(attr(deny(warnings))))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_favicon_url = "https://raw.githubusercontent.com/oxigraph/oxigraph/main/logo.svg")]
#![doc(html_logo_url = "https://raw.githubusercontent.com/oxigraph/oxigraph/main/logo.svg")]

pub use crate::optimizer::Optimizer;

pub mod algebra;
mod optimizer;
mod rdflib;
mod scope;
mod type_inference;

#[cfg(test)]
mod rdflib_lowering_tests;

mod source_identity;
pub use source_identity::implementation_fingerprint;
#[cfg(test)]
mod rdflib_filter_tests;
