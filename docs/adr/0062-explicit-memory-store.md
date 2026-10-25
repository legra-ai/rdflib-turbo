# ADR 0062: Explicit Memory constructor selection

Status: Accepted

Graph and Dataset constructors accept RDFLib's registered `Memory` spelling as
well as its `default` spelling. Both explicitly select the existing native
in-memory RDFLibStore owner. This is a storage choice, not a switch to RDFLib's
Python implementation. Explicit backend objects retain their existing ownership
and validation rules; unsupported backend names still fail immediately.
