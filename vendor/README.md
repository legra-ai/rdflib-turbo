# Source provenance

The oxrdf, sparopt, spareval and spargebra sources and their compatibility patches
are extracted unchanged from legra-ai/pyshacl-turbo commit `168b4ded09f36ce49a6bac6b6c91f660de2fd431`.
Original crate archive hashes are retained in rust-sources.json. Each directory
retains its upstream license files and patch notes. Build output is excluded.

The native inference implementation and RDF/list helpers in src are extracted
from the same commit under MIT OR Apache-2.0. This repository owns the extracted
implementation from this point forward; it has no Python dependency on pyshacl-turbo.
No unpublished preparation-cache changes were copied.

- `oxsdatatypes-0.2.3/`: published Oxigraph datatype sources with one parser correction
  for `xsd:gMonthDay` February 29. See its `TURBO-PATCH.md` and original archive
  checksum in `rust-sources.json`. MIT and Apache license texts are retained.
