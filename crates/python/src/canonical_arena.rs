//! Private exact typed records, not RDF text or a persistent transport format.

use pyshacl_turbo_engine::oxigraph::model::*;

/// Complete normalized records; no graph owner, per-triple index or term strings.
#[derive(PartialEq, Eq, Hash)]
pub(super) struct Records {
    bytes: Box<[u8]>,
    count: usize,
}

/// One exact source record, including graph placement and empty graph declarations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Record<'a> {
    Quad(GeneralizedQuadRef<'a>),
    NamedGraph(NamedOrBlankNodeRef<'a>),
}

/// Temporary storage follows the existing 256-record transfer boundary.
#[derive(Default)]
pub(super) struct Builder {
    active: Vec<u8>,
    active_count: usize,
    blocks: Vec<Block>,
    spans: Vec<Span>,
}

struct Block {
    bytes: Option<Box<[u8]>>,
    remaining: usize,
}

struct Span {
    block: usize,
    start: usize,
}

impl Block {
    fn record(&self, span: &Span) -> &[u8] {
        let bytes = self.bytes.as_deref().expect("pending shape input block");
        let input = &bytes[span.start..];
        let first = input[0];
        if first < 128 {
            return &input[1..1 + usize::from(first)];
        }
        let mut input = input;
        let length = decode_length(&mut input);
        &input[..length]
    }
}

impl Builder {
    pub(super) fn push_quad(&mut self, quad: GeneralizedQuadRef<'_>) {
        let terms = [quad.subject, quad.predicate, quad.object].map(EncodedTerm::new);
        let graph = match quad.graph_name {
            GraphNameRef::NamedNode(value) => Some(EncodedTerm::new(value.into())),
            GraphNameRef::BlankNode(value) => Some(EncodedTerm::new(value.into())),
            GraphNameRef::DefaultGraph => None,
        };
        let length = terms
            .iter()
            .map(EncodedTerm::len)
            .try_fold(1usize, usize::checked_add)
            .and_then(|size| size.checked_add(graph.as_ref().map_or(1, EncodedTerm::len)))
            .expect("shape quad byte length fits memory");
        self.begin_record(length);
        self.active.push(0); // Quad, independent of its graph-name domain.
        for term in terms {
            term.write(&mut self.active);
        }
        if let Some(graph) = graph {
            graph.write(&mut self.active);
        } else {
            self.active.push(5); // Explicit default graph, never an RDF term.
        }
    }

    pub(super) fn push_graph(&mut self, graph: NamedOrBlankNodeRef<'_>) {
        let graph = match graph {
            NamedOrBlankNodeRef::NamedNode(value) => EncodedTerm::new(value.into()),
            NamedOrBlankNodeRef::BlankNode(value) => EncodedTerm::new(value.into()),
        };
        self.begin_record(
            graph
                .len()
                .checked_add(1)
                .expect("shape graph record fits memory"),
        );
        self.active.push(1); // Graph declaration, independent of quad records.
        graph.write(&mut self.active);
    }

    fn begin_record(&mut self, length: usize) {
        if self.active_count == super::BATCH_SIZE {
            self.finish_batch();
        }
        let start = self.active.len();
        encode_length(&mut self.active, length);
        self.spans.push(Span {
            block: self.blocks.len(),
            start,
        });
        self.active_count += 1;
    }

    pub(super) fn finish_batch(&mut self) {
        if self.active_count == 0 {
            return;
        }
        self.blocks.push(Block {
            bytes: Some(std::mem::take(&mut self.active).into_boxed_slice()),
            remaining: 0,
        });
        self.active_count = 0;
    }

    pub(super) fn finish(mut self) -> Records {
        self.finish_batch();
        let blocks = &self.blocks;
        self.spans.sort_unstable_by(|left, right| {
            blocks[left.block]
                .record(left)
                .cmp(blocks[right.block].record(right))
                .then_with(|| (left.block, left.start).cmp(&(right.block, right.start)))
        });
        // Equal bytes are adjacent; retain the earliest physical witness.
        // The tie-break never changes canonical record bytes or their ordering.
        self.spans.dedup_by(|left, right| {
            blocks[left.block].record(left) == blocks[right.block].record(right)
        });
        let size = self
            .spans
            .iter()
            .map(|span| self.blocks[span.block].record(span).len())
            .sum();
        for span in &self.spans {
            self.blocks[span.block].remaining += 1;
        }
        for block in &mut self.blocks {
            if block.remaining == 0 {
                block.bytes = None;
            }
        }
        let count = self.spans.len();
        let mut bytes = Vec::with_capacity(size);
        for span in self.spans {
            let block = &mut self.blocks[span.block];
            bytes.extend_from_slice(block.record(&span));
            block.remaining -= 1;
            if block.remaining == 0 {
                block.bytes = None;
            }
        }
        Records {
            bytes: bytes.into_boxed_slice(),
            count,
        }
    }
}

impl Records {
    pub(super) fn len(&self) -> usize {
        self.count
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = Record<'_>> {
        let mut input = self.bytes.as_ref();
        (0..self.count).map(move |_| decode_record(&mut input))
    }

    #[cfg(test)]
    pub(super) fn retained_bytes(&self) -> usize {
        self.bytes.len()
    }
}

struct EncodedTerm<'a> {
    tag: u8,
    value: &'a str,
    qualifier: Option<&'a str>,
}

impl<'a> EncodedTerm<'a> {
    fn new(term: TermRef<'a>) -> Self {
        let (tag, value, qualifier) = match term {
            TermRef::NamedNode(value) => (0, value.as_str(), None),
            TermRef::BlankNode(value) => (1, value.as_str(), None),
            TermRef::Literal(value) => {
                if let Some(language) = value.language() {
                    (4, value.value(), Some(language))
                } else if value.has_explicit_datatype() {
                    (3, value.value(), Some(value.datatype().as_str()))
                } else {
                    (2, value.value(), None)
                }
            }
        };
        Self {
            tag,
            value,
            qualifier,
        }
    }

    fn len(&self) -> usize {
        let field = |text: &str| {
            text.len()
                .checked_add(length_size(text.len()))
                .expect("shape field byte length fits memory")
        };
        1usize
            .checked_add(field(self.value))
            .and_then(|size| size.checked_add(self.qualifier.map_or(0, field)))
            .expect("shape term byte length fits memory")
    }

    fn write(&self, output: &mut Vec<u8>) {
        output.push(self.tag);
        encode_text(output, self.value);
        if let Some(qualifier) = self.qualifier {
            encode_text(output, qualifier);
        }
    }
}

fn length_size(mut length: usize) -> usize {
    let mut size = 1;
    while length >= 128 {
        length >>= 7;
        size += 1;
    }
    size
}

fn encode_length(output: &mut Vec<u8>, mut length: usize) {
    while length >= 128 {
        output.push((length as u8 & 127) | 128);
        length >>= 7;
    }
    output.push(length as u8);
}

fn encode_text(output: &mut Vec<u8>, text: &str) {
    encode_length(output, text.len());
    output.extend_from_slice(text.as_bytes());
}

fn decode_record<'a>(input: &mut &'a [u8]) -> Record<'a> {
    match take_tag(input) {
        0 => Record::Quad(GeneralizedQuadRef {
            subject: decode_term(input),
            predicate: decode_term(input),
            object: decode_term(input),
            graph_name: decode_graph(input),
        }),
        1 => Record::NamedGraph(match decode_graph(input) {
            GraphNameRef::NamedNode(value) => value.into(),
            GraphNameRef::BlankNode(value) => value.into(),
            GraphNameRef::DefaultGraph => unreachable!("named graph declaration"),
        }),
        _ => unreachable!("privately constructed shape record tag"),
    }
}

fn decode_graph<'a>(input: &mut &'a [u8]) -> GraphNameRef<'a> {
    match take_tag(input) {
        0 => NamedNodeRef::new_unchecked(decode_text(input)).into(),
        1 => BlankNodeRef::new_unchecked(decode_text(input)).into(),
        5 => GraphNameRef::DefaultGraph,
        _ => unreachable!("privately constructed shape graph tag"),
    }
}

fn take_tag(input: &mut &[u8]) -> u8 {
    let (tag, rest) = input.split_first().expect("complete typed shape record");
    *input = rest;
    *tag
}

fn decode_term<'a>(input: &mut &'a [u8]) -> TermRef<'a> {
    // These records are privately constructed from already validated RDF terms.
    let tag = take_tag(input);
    let value = decode_text(input);
    match tag {
        0 => NamedNodeRef::new_unchecked(value).into(),
        1 => BlankNodeRef::new_unchecked(value).into(),
        2 => LiteralRef::new_simple_literal(value).into(),
        3 => LiteralRef::new_typed_literal(value, NamedNodeRef::new_unchecked(decode_text(input)))
            .into(),
        4 => LiteralRef::new_language_tagged_literal_unchecked(value, decode_text(input)).into(),
        _ => unreachable!("privately constructed shape term tag"),
    }
}

fn decode_length(input: &mut &[u8]) -> usize {
    let mut length = 0usize;
    let mut shift = 0;
    loop {
        let (byte, rest) = input.split_first().expect("complete shape field length");
        *input = rest;
        length |= usize::from(byte & 127) << shift;
        if byte & 128 == 0 {
            break;
        }
        shift += 7;
    }
    length
}

fn decode_text<'a>(input: &mut &'a [u8]) -> &'a str {
    let length = decode_length(input);
    let (text, rest) = input.split_at(length);
    *input = rest;
    std::str::from_utf8(text).expect("validated shape UTF8 field")
}

#[cfg(test)]
#[path = "canonical_arena_tests.rs"]
mod tests;
