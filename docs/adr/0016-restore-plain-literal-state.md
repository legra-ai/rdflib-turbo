# ADR 0016: Restore plain literal state at the native boundary

Status: Accepted

Native reads already contain accepted lexical strings and validated language
tags. Re-running RDFLib's general input constructor for every plain literal
repeats type dispatch and lexical conversion. Reference stores return existing
Literal objects instead, making this a substantial adapter cost.

Centralize plain/language-literal restoration for typed and JSON native outputs.
Allocate the actual RDFLib Literal string subclass and initialize its existing
language, datatype, Python-value and ill-typed slots. A literal without a datatype
has its lexical string as its Python value and no ill-typed status. Preserve all
lexical characters. No subclass, global cache, reference graph or RDF mirror is
introduced. Typed literals continue through RDFLib's general constructor and
datatype conversion registry.

This deliberately couples native restoration to RDFLib's Literal state layout.
Tests must check the actual class and public value, language, datatype,
ill-typed status, hashing, serialization and pickle behavior when updating the
RDFLib dependency. Unsupported layout changes must fail rather than silently
constructing a different term representation.

The regression first prohibits redundant plain-string casting, then checks
empty strings, whitespace, Unicode, long lexicals and language literals against
RDFLib-created terms. Complete installed-wheel tests and independent workload
comparisons remain the release evidence.
