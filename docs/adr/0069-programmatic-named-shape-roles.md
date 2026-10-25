# ADR 0069: Retain programmatic named shape role identity

Status: Accepted for the private exact-Git model prototype; installed acceptance pending.

Original RDFLib URIRef shape roles preserve empty, relative, path and
space-containing lexical values. The native reader correctly checked their
NamedNode variant, then incorrectly reparsed it as an absolute RFC IRI. The
published model previously fixed these roles to that stricter syntax type.
Changing the transfer decoder alone cannot repair this boundary.

Use the shared model's caller-owned named-role parameter with Native NamedNode.
Keep ordinary shape values, counts and flags as Term, and explicitly retain
String as the fourth pattern parameter. Centralize these choices in model.rs.
Named-only readers still reject literal/blank variants, but transfer an existing
NamedNode directly without normalization, reparsing, sentinels or a side map.
Strict model defaults, syntax/data decoders and severity policy remain separate.

The prototype pins the real public model Git revision
572333b3455acab7a5ca6b0bbb68f712ebaa68b6 in both the manifest and lock. The source
reports model version 6.0.0; this does not assert that registry version contains
this API. Public consumer publication must use an independently accepted actual
Git revision or a subsequently verified registry archive. No silent local patch
or guessed future minimum version is permitted.

Keep full original/native report graph and text comparisons. Programmatic
datatype roles also exposed a missing datatype-specific diagnostic; use the
compiled datatype identity in the original message rather than a generic
constraint label. Preserve shape-only explicit empty literal datatypes without
changing the ordinary data transfer policy. Test this at its own input boundary.

The engine fingerprint includes the actual manifest and local alias/reader
sources. Record the resolved public Git dependency source in artifact receipts.
Declaration-only alias modules require exact source hashes and the existing
missing-executable-mapping checks, with no coverage threshold relaxation.
Source tests establish a bounded role fix, not universal parity or performance.

## Promote the merged structural source

Use merged source revision 47357fe16b6fdf3ef610534384e26fed62a4bb54
(model 6.0.2), replacing the unmerged 572333 revision above. Its named-role
generics are unchanged. Resolve only its exact iri-rfc3987 0.1.7,
rdf-structural-model 0.2.2 and rhusky 2.0.1 dependencies and required transitives;
retain unrelated locked versions.

The added typed `Component::SingleLine(bool)` is explicitly unsupported by this
compatibility runtime. Do not silently weaken a typed declaration into no
constraint. Public RDF `sh:singleLine` remains an unknown predicate, as in the
pinned pySHACL 0.40.1 reference: true, false and malformed values are ignored for
both node and property shapes. A model dependency update does not authorize
adopting SHACL 1.2 execution semantics absent from that reference.

## Preserve the published SingleLine extension

The public pyshacl-turbo 0.7.0 feature supersedes the preceding private decision
to ignore SingleLine. Use the actually published structural model 6.1.0, pinned
exactly through the registry and original Cargo checksum. Retain native NamedNode
roles and original RDF pattern Terms until regex compilation, including the
existing interpreter-owned host policy.

Lower `SingleLine(true)` to native scalar validation and omit the disabled false
case. Literal values containing U+000A through U+000D violate the selected draft
constraint; non-literals and other Unicode separators do not. Invalid boolean or
repeated declarations fail compilation. Reports preserve the original RDF value
and SingleLineConstraintComponent identity.

Keep the public Rust and Python regressions, adapting only the canonical indexed
shape input API. Original pySHACL 0.40.1 comparisons explicitly establish this
selected SHACL 1.2 draft extension instead of asserting identical behavior or
skipping it. This does not establish complete SHACL 1.2 conformance.
