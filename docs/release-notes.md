First functional alpha of rdflib-turbo.

Native exact-term RDF Dataset with parsing, serialization, SELECT/ASK/CONSTRUCT/DESCRIBE, staged inference and explicit RDFLib conversion. Five platform ABI3 wheels support Python 3.11+.

The dataset runtime is shared with [owlrl-turbo](https://github.com/legra-ai/owlrl-turbo). [pyshacl-turbo](https://github.com/legra-ai/pyshacl-turbo) currently integrates through explicit copying conversion.

This is a limited alpha API, not a complete drop-in replacement. See the README and ADRs for scope, intentional reference differences and remaining work. No performance claim is made.
