# ADR 0044: Accept Dataset reports before applying rule output

Status: Accepted

Legacy Dataset validation must keep rule additions private through consumer
warning policy and report decoding, matching the existing source context used by
Store validation. A decoding exception must not leave additions committed merely
because native execution returned a successful report.

A frozen prepared result retains its native Dataset owner, an owned typed report,
private derived-only additions, and the original mutation revision. The existing
Python source context consumes the report once, lets the consumer emit recursion
warnings and decode/bind it, then accepts the revision and applies additions.
Report transport stays unchanged; mutating the returned report cannot alter the
separately owned native delta. Abandonment or consumer errors discard that delta
without touching the source. Acceptance and output consumption are single-use.

Use the existing Dataset revision comparison for both prepared acceptance and
immediate warning-before-commit primitives. Intervening writes, including no-op
mutations, cause a hard conflict without retries. Unlike an explicit Store
transaction, independently committed Dataset callback writes survive the failed
validation. A successful acceptance never overwrites namespaces or original data.

Validation borrows its original indexed Dataset through a read lease while
staging. Read-only acceptance checks its revision through a shared read lease;
it must not request an exclusive lock or advance the mutation counter. Active
inplace rules accept under the existing write lock. Neither phase copies the
input or holds a native lock across Python callbacks.

The direct SharedShapes.validate and rule-expansion primitives retain their
immediate semantics and existing warning policy. Only source-context preparation
returns recursion messages for the consumer to emit once at its existing caller
location. No duplicate facade adapter, engine revision, feature selection or
integration protocol change is introduced.
