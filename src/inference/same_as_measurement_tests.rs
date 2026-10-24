//! Shared measured full-Term and strict equality query views.
use super::*;
use serde::Deserialize;

pub(super) struct Measured<'a> {
    pub(super) data: &'a GeneralizedDataset,
    pub(super) work: Cell<usize>,
}

#[derive(Deserialize)]
struct Explanation {
    plan: Node,
}
#[derive(Deserialize)]
struct Node {
    #[serde(rename = "number of results")]
    rows: usize,
    children: Vec<Node>,
}
impl Node {
    fn work(&self) -> usize {
        self.rows + self.children.iter().map(Self::work).sum::<usize>()
    }
}

impl<'data> Graph for Measured<'data> {
    fn quads<'a>(
        &'a self,
        s: Option<TermRef<'_>>,
        p: Option<TermRef<'_>>,
        o: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'data> {
        self.work.set(self.work.get() + 1);
        self.data
            .quads_for_pattern(s, p, o, Some(GraphNameRef::DefaultGraph))
            .map(|q| {
                self.work.set(self.work.get() + 1);
                Ok(q.into_owned())
            })
    }
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        let (result, explanation) = query
            .clone()
            .on_queryable_dataset(self.data)
            .compute_statistics()
            .explain();
        let QueryResults::Graph(triples) = result.unwrap() else {
            panic!("expected CONSTRUCT")
        };
        for triple in triples {
            triple.unwrap();
        }
        // bounded: the fixed rule's explanation tree, independent of graph cardinality.
        let mut bytes = Vec::new();
        explanation.write_in_json(&mut bytes).unwrap();
        let explanation: Explanation = serde_json::from_slice(&bytes).unwrap();
        self.work.set(self.work.get() + explanation.plan.work());
        Ok(query.on_queryable_dataset(self.data).execute()?)
    }

    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        let (result, explanation) = query
            .clone()
            .on_queryable_dataset(self.data)
            .compute_statistics()
            .explain_inference_construct();
        let triples = result.unwrap();
        for triple in triples {
            triple.unwrap();
        }
        // bounded: the fixed rule's explanation tree, independent of graph cardinality.
        let mut bytes = Vec::new();
        explanation.write_in_json(&mut bytes).unwrap();
        let explanation: Explanation = serde_json::from_slice(&bytes).unwrap();
        self.work.set(self.work.get() + explanation.plan.work());
        Ok(query
            .on_queryable_dataset(self.data)
            .execute_inference_construct()?)
    }
    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        panic!("immutable kernel fixture must not postprocess storage")
    }

    fn insert(&mut self, _: GeneralizedQuad) -> Result<()> {
        panic!("round must not mutate its read snapshot")
    }
}
