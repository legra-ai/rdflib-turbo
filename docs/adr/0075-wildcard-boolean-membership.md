# ADR 0075: Stop wildcard boolean membership at its first match

Status: Accepted

Ordinary committed Graph membership with an unbound term used the projected-row
reader. One boolean query could restore 128 Literal objects before returning.
Use the existing typed membership boundary for both bound and wildcard patterns.
Pin only supplied terms, detach for the scoped native lookup, and return a boolean
without a result batch, RDF term factories or a continuation. Keep the direct
exact-quad lookup for fully bound patterns; wildcard lookups stop at the first
matching generalized quad and propagate read failures. Project no result fields
so even the first matching stored literal is not decoded or copied. The isolated
CI allocation budget checks a 4 MiB stored literal with an unbound object.

Preserve the existing dispatch for overridden triples, instance callbacks,
property paths and transaction owners. Scope and exact lexical identity retain
their existing contracts. No cache, alternate storage, input serialization or
Python RDF mirror is introduced.
