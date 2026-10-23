# ADR 0052: Stage private inference before native validation

Status: accepted; implementation in progress.

Native validate(inference=...) must infer against the same borrowed source that
SHACL reads, including exact graph scope and pending caller writes. It must not
pre-call public inference, copy the base into another store, serialize input, or
reserve a second writer for committed read-only validation.

Keep only derived inference facts in a provider-owned indexed overlay. Compose
its logical default union and original graph contexts explicitly for reasoner
queries and the existing engine GraphRead/QueryGraph/validate_delta interfaces.
Do not copy Dataset-index adjacent deduplication into Store iteration: stable
exact representative probes must preserve RDF-set semantics without assuming
storage order or retaining a source-sized visited set.

The private reasoner Graph insertion boundary returns Result. Testing whether a
derived quad already belongs to a borrowed source can fail. Propagate insertion
errors through axioms, literal substitution, rule rounds and diagnostic nodes;
never swallow a read error, retain a sticky error, or publish a partial result.
Existing native transactions/journals keep rollback ownership.

Inference and SHACL rule additions have distinct physical destinations. Stage
both until caller-thread lexical signal publication, warning policy and report
decoding succeed, then use the existing session revision acceptance. Preserve
source and namespace state on non-inplace success; failures still abort an active
explicit transaction. Selected default/IRI/blank-node Graph views cannot read
unrelated contexts. Dataset validation retains dataset-union semantics and the
inference destination contract.

Carry strict optional inference settings independently from engine Options.
Ontology inoculation and native rule-only pre-inference remain explicitly
unsupported. Protocol remains 1. This provider/consumer change reuses canonical
engine APIs and requires no engine or vendor source change; fingerprints and
registry-pair guards remain binding.

Graph validation captures authoritative data namespace pairs separately from
shape namespace pairs and transports them through strict typed settings. This
also preserves ordinary non-inference report formatting. Diagnostic inference
binds its error namespace privately for report construction; inplace acceptance
publishes that metadata with the derived facts after the existing namespace and
RDF revision checks. Prefix collisions and rebinding follow RDFLib, including
replacing an existing alias of the diagnostic namespace. Keep the report output
four-tuple unchanged; internal prepared-result metadata is separate.

Initial regressions verify the public option rejection and a fallible insertion
contract covering axiom seeding, native rounds, literal substitution, diagnostics
and rollback. Borrowed query lifetime, source-read failures, scope, state,
allocation and acceptance regressions are required before exposing the option.
