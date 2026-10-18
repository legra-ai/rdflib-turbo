# ADR 0022: Resolve graph view registration through the native graph-name index

Status: Implemented locally; not released

Creating or reopening a StoreDataset graph view enumerated every named graph and
counted the default graph through `contexts()`. This made a view lookup depend on
the size of unrelated data and allocated a Python set of graph identifiers.

Ask the existing native named-graph index whether the requested name exists.
Register only an absent graph. Do not enumerate contexts, count statements, or
maintain a Python graph-name cache. Empty named graphs remain registered and
reopening a populated graph never replaces its statements. Relative URIRef and
blank-node identifiers use the same exact-term parser as graph mutation.

Store membership reads committed state without reserving a writer, including
while an explicit transaction is active. Transaction membership runs on its
owner thread and sees staged registration and removal. Owner-side validation
errors abort the transaction even when Python catches the error. Closed handles
fail through the same Store/session lifecycle checks as other native operations.

Installed-wheel regressions first fail on context enumeration and the missing
native membership API. They preserve reference RDFLib behavior for empty and
repeated registration, exercise transaction isolation and abort, and verify
closed handles. This change does not establish overall performance superiority.
