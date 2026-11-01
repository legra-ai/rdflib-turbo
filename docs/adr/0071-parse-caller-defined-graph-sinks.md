# ADR 0071: Parse caller-defined Graph sinks through their actual callbacks

Status: Accepted

A public downstream Graph subclass records first-seen subject order in its
`add` override. The original RDFLib parser calls that actual object for each
statement. Native Graph parsing instead creates a base Graph over an insertion
transaction; the stored triples agree, but the override never runs. A callback
that raises is also silently bypassed. Equal RDF output is insufficient here.

For a caller-defined subclass of the native Graph, or a Graph instance with its
own `add` callback, invoke RDFLib's parser with the actual instance as its sink.
An instance callback may be a plain callable or an explicitly bound method;
the receiver and every insertion must remain observable. Choose this source
capability before parsing; never retry after a native parser failure. The parser writes to the
same native Store through the subclass's ordinary methods. It does not allocate
a reference Memory graph, copy the input relation, replace the caller's store,
or use a proxy with a different identity. RDFLib owns callback, namespace and
parser exception ordering for that source sink.

This narrows ADR 0009's implicit atomic-document guarantee: a committed
caller-defined Graph sink follows RDFLib's incremental insertion behavior,
including earlier accepted statements when a later callback fails. Closed
native Graph and Dataset parsing retain their existing native transaction and
rollback semantics. A subclass explicitly bound to a native transaction still
aborts that transaction if parsing or a callback raises, including a failure
caught by the Python caller before context exit. Dataset subclass parsing and
original-import replacement are separate contracts; this decision does not
claim to close them.

Keep regressions against both actual reference Graph and native Graph for
insertion order, actual sink identity, callback exception identity and partial
input, including instance-installed callbacks. Retain independent controls for
closed-native late syntax rollback and caught errors inside explicit
transactions. Verify installed wheels and the
public downstream suite before claiming compatibility or performance results.
