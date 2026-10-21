# ADR 0043: Accept staged validation on the transaction owner

Status: Accepted

SHACL validation on a Store transaction borrows its existing indexed base and
pending caller writes. The engine returns an owned report and derived-only rule
delta through validate_delta. The native transaction worker retains that delta
privately while Python applies warning policy and decodes the report inside the
existing source context. Reentrant reads see caller writes, never unaccepted
rule output. No base graph is copied, serialized or held behind a Python callback
lock. Non-inplace committed Store reads retain their pinned MVCC read view.

A prepared result owns its session and expected revision. Successful context exit
accepts that revision on the same owner thread and applies the delta. Every native
mutation attempt advances the per-session revision before execution, including
no-op updates, graph metadata operations, loads, and inference. A native error
already aborts the worker and all its pending writes, including partially parsed
loads. Independent sessions do not share counters. Read operations do not advance
revisions. A second preparation or commit while a report is outstanding is an
error, rather than an implicit acceptance.

Warning callbacks and other report consumers run without session locks. They may
read but cannot change the transaction accepted by that validation. An intervening
mutation causes a hard conflict and aborts the entire transaction, even if the
mutation made no RDF change. There are no retries. Consumer exceptions, abandoned
prepared results, warning-as-error, and report decoding failures also abort the
owner. Python continues to emit warnings with its existing category and caller
location; the provider does not duplicate them.

Direct validation accepts successful nonconformance. The optional transaction
shapes gate separately rejects nonconformance and aborts original and derived
writes together. Low-level immediate native validation retains its immediate
acceptance contract; the shared consumer source context uses preparation and
acceptance explicitly. This extends the existing internal protocol 1, with no
engine fingerprint change or new public consumer protocol.
