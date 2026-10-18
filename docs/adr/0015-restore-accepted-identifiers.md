# ADR 0015: Restore accepted identifiers without repeating input validation

Status: Accepted

RDFLib accepts some URIRef spellings with a construction-time warning. Memory
stores return the accepted object when reading; they do not warn again. The
native facade called URIRef's input constructor on every read, repeating warnings
and the URI validation work for every materialized term.

Native output already contains accepted identifier strings. Restore URIRef and
BNode with their inherited string allocator, producing the actual RDFLib classes
without invoking their Python input constructors again. These identifier classes
have no additional instance state. Apply the same restoration to typed and legacy
JSON native outputs. This does not change input acceptance, parser validation,
namespace binding or lexical spelling. Literal datatype conversion continues to
use RDFLib's constructor.

The regression first demonstrates repeated warnings, then verifies their absence
and exact classes/values through ordinary and transactional Graph reads. Measure
installed-wheel workloads before claiming a performance improvement.
