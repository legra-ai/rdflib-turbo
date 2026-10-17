# Source provenance

The oxrdf, sparopt, spareval and spargebra sources and their compatibility patches
are extracted unchanged from legra-ai/pyshacl-turbo commit `168b4ded09f36ce49a6bac6b6c91f660de2fd431`.
Original crate archive hashes are retained in rust-sources.json. Each directory
retains its upstream license files and patch notes. Build output is excluded.

The native inference implementation and RDF/list helpers in src are extracted
from the same commit under MIT OR Apache-2.0. This repository owns the extracted
implementation from this point forward; it has no Python dependency on pyshacl-turbo.
No unpublished preparation-cache changes were copied.

The rdflib-turbo runtime additionally patches oxrdf's dataset/interner with
additive insertion transactions; see `oxrdf-0.3.4/TURBO-PATCH.md` and ADR 0005.
The initial extraction provenance above does not imply this new patch is present
in the companion pyshacl-turbo checkout.

- `oxsdatatypes-0.2.3/`: published Oxigraph datatype sources with one parser correction
  for `xsd:gMonthDay` February 29. See its `TURBO-PATCH.md` and original archive
  checksum in `rust-sources.json`. MIT and Apache license texts are retained.

- `oxigraph-0.5.11/`: published crate with opt-in exact typed-literal storage
  and SPARQL term identity patches. Its `TURBO-PATCH.md` describes the changes;
  `rust-sources.json` records its own archive checksum and upstream revision.
  Both upstream license texts are retained. These new patches and spareval's
  `exact-term-builtins` patch originate here, not in the initial extraction.

- `oxrocksdb-sys-0.5.11-notices/`: unmodified Apache, LevelDB and LZ4 library
  notices for the registry dependency linked into persistent-capable wheels.
  Its README records archive identity and bundled components. These are license
  files only; Cargo.lock pins the unmodified registry crate.
