"""Explicit JSON-LD option dispatch over the caller's native transaction sink."""

from inspect import getattr_static

from rdflib import plugin
from rdflib.parser import Parser

# bounded: the seven named options in RDFLib's JSON-LD parser contract.
JSONLD_OPTIONS = frozenset(
    {
        "version",
        "context",
        "base",
        "skolemize",
        "encoding",
        "generalized_rdf",
        "extract_all_scripts",
    }
)
# bounded: the native alias and the two RDFLib JSON-LD format names.
JSONLD_FORMATS = frozenset({"json-ld", "jsonld", "application/ld+json"})


def _context_metadata(source):
    # Inspect capability without evaluating descriptors or consuming link iterators.
    try:
        getattr_static(source, "url")
        getattr_static(source, "links")
    except AttributeError:
        return getattr_static(type(source), "__getattr__", None) is not None
    return True


class JsonLdOptions:
    def __init__(self, rdf_format, options, base):
        self.base = base
        self.is_jsonld = rdf_format in JSONLD_FORMATS
        self.format = "json-ld" if self.is_jsonld else rdf_format
        self.plugin_format = "json-ld" if rdf_format == "jsonld" else rdf_format
        self.requested_options = options
        self.plugin_options = None
        if not options:
            return
        if not self.is_jsonld:
            raise TypeError(f"Unsupported native parser options: {sorted(options)}")
        unknown = options.keys() - JSONLD_OPTIONS
        if unknown:
            raise TypeError(f"Unsupported JSON-LD parser options: {sorted(unknown)}")
        if options.get("generalized_rdf"):
            raise NotImplementedError("Generalized RDF predicates are unsupported")
        version = float(options.get("version", 1.1))
        if version not in {1.0, 1.1}:
            raise NotImplementedError(f"Unsupported JSON-LD version: {version}")
        self.format = "json-ld"
        self.base = options.get("base") or base
        # The native parser already implements the default 1.1 mode and base IRI.
        if version == 1.1 and options.keys() <= {"version", "base"}:
            return
        self.plugin_options = options

    def parse_plugin(self, source, sink):
        options = self.plugin_options
        if options is None:
            if not self.is_jsonld or not _context_metadata(source):
                return False
            options = self.requested_options
        # Selected option capability, never a retry after a native parser error.
        plugin.get(self.plugin_format, Parser)().parse(source, sink, **options)
        return True
