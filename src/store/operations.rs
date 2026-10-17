use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::sparql::SparqlEvaluator;
use oxigraph::store::Transaction;

use crate::{Error, Result};

use super::output::{dump, prepare_query, query_output};

pub(super) enum Operation {
    Len,
    Load {
        source: String,
        format: RdfFormat,
        base: Option<String>,
    },
    Update(String),
    Query(String, bool),
    Dump(RdfFormat),
}

pub(super) enum Reply {
    Done,
    Count(usize),
    Text(String),
    Query(String, String),
}

impl Operation {
    pub fn execute(self, transaction: &mut Transaction<'_>) -> Result<Reply> {
        match self {
            Self::Len => transaction.len().map(Reply::Count).map_err(runtime),
            Self::Load {
                source,
                format,
                base,
            } => {
                let mut parser = RdfParser::from_format(format).rename_blank_nodes();
                if let Some(base) = base {
                    parser = parser.with_base_iri(base).map_err(invalid)?;
                }
                for quad in parser.for_slice(source.as_bytes()) {
                    transaction.insert(&quad.map_err(invalid)?);
                }
                Ok(Reply::Done)
            }
            Self::Update(source) => {
                SparqlEvaluator::new()
                    .parse_update(&source)
                    .map_err(invalid)?
                    .on_transaction(transaction)
                    .execute()
                    .map_err(runtime)?;
                Ok(Reply::Done)
            }
            Self::Query(source, union) => {
                let (query, kind) = prepare_query(&source, union)?;
                let results = query.on_transaction(transaction).execute()?;
                let (kind, output) = query_output(results, kind)?;
                Ok(Reply::Query(kind, output))
            }
            Self::Dump(format) => dump(transaction.iter(), format).map(Reply::Text),
        }
    }
}

pub(super) fn invalid(error: impl std::fmt::Display) -> Error {
    Error::Invalid(error.to_string())
}

pub(super) fn runtime(error: impl std::fmt::Display) -> Error {
    Error::Runtime(error.to_string())
}
