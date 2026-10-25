//! Borrowed range selection over the shared six indexes; no decoded row cache.
use super::*;
use std::collections::btree_set::Range;

// bounded: four encoded filter terms and one iterator, independent of dataset size.
struct Pattern {
    s: Option<InternedTerm>,
    p: Option<InternedTerm>,
    o: Option<InternedTerm>,
    g: Option<InternedGraphName>,
    order: [usize; 4],
    prefix: usize,
}
impl Pattern {
    fn new(
        data: &GeneralizedDataset,
        s: Option<TermRef<'_>>,
        p: Option<TermRef<'_>>,
        o: Option<TermRef<'_>>,
        g: Option<GraphNameRef<'_>>,
    ) -> Option<Self> {
        let i = &data.data.interner;
        let s = s
            .map(|t| InternedTerm::encoded_from(t, i))
            .transpose_option()?;
        let p = p
            .map(|t| InternedTerm::encoded_from(t, i))
            .transpose_option()?;
        let o = o
            .map(|t| InternedTerm::encoded_from(t, i))
            .transpose_option()?;
        let g = g
            .map(|t| InternedGraphName::encoded_from(t, i))
            .transpose_option()?;
        let order = if g.is_some() {
            if s.is_some() {
                [3, 0, 1, 2]
            } else if p.is_some() {
                [3, 1, 2, 0]
            } else {
                [3, 2, 0, 1]
            }
        } else if s.is_some() {
            [0, 1, 2, 3]
        } else if p.is_some() {
            [1, 2, 0, 3]
        } else {
            [2, 0, 1, 3]
        };
        let present = [s.is_some(), p.is_some(), o.is_some(), g.is_some()];
        let prefix = order.iter().take_while(|&&field| present[field]).count();
        Some(Self {
            s,
            p,
            o,
            g,
            order,
            prefix,
        })
    }
    fn same(&self, record: &QuadRecord, field: usize) -> bool {
        let (s, p, o, g) = record.generalized_parts();
        match field {
            0 => self.s.as_ref().is_none_or(|t| s == t.into()),
            1 => self.p.as_ref().is_none_or(|t| p == t.into()),
            2 => self.o.as_ref().is_none_or(|t| o == t.into()),
            3 => self.g.as_ref().is_none_or(|t| g == t),
            _ => unreachable!(),
        }
    }
    fn prefix_matches(&self, record: &QuadRecord) -> bool {
        self.order[..self.prefix]
            .iter()
            .all(|&f| self.same(record, f))
    }
    fn matches(&self, record: &QuadRecord) -> bool {
        (0..4).all(|f| self.same(record, f))
    }
    fn range<'a>(&self, data: &'a Dataset) -> Records<'a> {
        let record = Arc::new(QuadRecord::from_generalized((
            self.s.clone().unwrap_or_else(InternedTerm::first),
            self.p.clone().unwrap_or_else(InternedTerm::first),
            self.o.clone().unwrap_or_else(InternedTerm::first),
            self.g.unwrap_or_else(InternedGraphName::first),
        )));
        match self.order {
            [3, 0, 1, 2] => Records::Gspo(data.gspo.range(GspoKey::new(record)..)),
            [3, 1, 2, 0] => Records::Gpos(data.gpos.range(GposKey::new(record)..)),
            [3, 2, 0, 1] => Records::Gosp(data.gosp.range(GospKey::new(record)..)),
            [0, 1, 2, 3] => Records::Spog(data.spog.range(SpogKey::new(record)..)),
            [1, 2, 0, 3] => Records::Posg(data.posg.range(PosgKey::new(record)..)),
            [2, 0, 1, 3] => Records::Ospg(data.ospg.range(OspgKey::new(record)..)),
            _ => unreachable!(),
        }
    }
}
// Option<Option<T>> distinguishes an unbound role from a bound unknown term.
trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}
impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            None => Some(None),
            Some(Some(t)) => Some(Some(t)),
            Some(None) => None,
        }
    }
}
enum Records<'a> {
    Empty,
    Gspo(Range<'a, GspoKey>),
    Gpos(Range<'a, GposKey>),
    Gosp(Range<'a, GospKey>),
    Spog(Range<'a, SpogKey>),
    Posg(Range<'a, PosgKey>),
    Ospg(Range<'a, OspgKey>),
}
impl<'a> Iterator for Records<'a> {
    type Item = &'a Arc<QuadRecord>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Gspo(i) => i.next().map(|q| &q.record),
            Self::Gpos(i) => i.next().map(|q| &q.record),
            Self::Gosp(i) => i.next().map(|q| &q.record),
            Self::Spog(i) => i.next().map(|q| &q.record),
            Self::Posg(i) => i.next().map(|q| &q.record),
            Self::Ospg(i) => i.next().map(|q| &q.record),
        }
    }
}
pub(super) fn query<'a>(
    data: &'a GeneralizedDataset,
    s: Option<TermRef<'_>>,
    p: Option<TermRef<'_>>,
    o: Option<TermRef<'_>>,
    g: Option<GraphNameRef<'_>>,
) -> impl Iterator<Item = GeneralizedQuadRef<'a>> + use<'a> {
    let pattern = Pattern::new(data, s, p, o, g);
    let records = pattern
        .as_ref()
        .map_or(Records::Empty, |p| p.range(&data.data));
    records
        .map_while(move |record| {
            pattern
                .as_ref()
                .filter(|p| p.prefix_matches(record))
                .map(|p| (record, p.matches(record)))
        })
        .filter_map(|(record, matches)| matches.then_some(record))
        .map(|record| data.decode(record))
}
