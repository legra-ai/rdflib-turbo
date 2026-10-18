Functional alpha of rdflib-turbo.

Native exact-term RDF Dataset with parsing, serialization, SELECT/ASK/CONSTRUCT/DESCRIBE, atomic inference and explicit RDFLib conversion. Twenty-five platform wheels cover CPython 3.11 through 3.15, with a separate native wheel for each interpreter and platform.

The dataset runtime is shared with [owlrl-turbo](https://github.com/legra-ai/owlrl-turbo) and supports borrowed native SHACL validation through [pyshacl-turbo](https://github.com/legra-ai/pyshacl-turbo). Default-graph inference journals additions instead of cloning all six indexes, and retains only distinct new statements per round. Failure atomicity, graph destinations and exact term identity are preserved. An opt-in phase heap profiler and a required native allocation budget accompany the installed-wheel pipeline benchmarks.

This is a limited alpha API, not a complete drop-in replacement. See the README and ADRs for scope, intentional reference differences and remaining work. No performance claim is made.

Describes pyshacl-turbo, rdflib-turbo and owlrl-turbo only as companion turbo
projects. They are independent of the Legra platform. Reuse of public legra-ai
repositories does not imply platform membership.
