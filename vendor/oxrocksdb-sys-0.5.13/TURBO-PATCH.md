# Local build dependency selection

The RocksDB build uses bindgen's default `Rustfmt` formatter. Disable bindgen's
unused `prettyplease` default feature while retaining logging and runtime
libclang loading. The generated C ABI and selected formatter are unchanged.

Bindgen 0.73.2 accepts syn 2 or 3 and prettyplease 0.2 or 0.3 independently.
Cargo can consequently select syn 2 for bindgen while prettyplease 0.3 requires
syn 3. Their incompatible AST types fail compilation even though RocksDB never
uses the optional formatter. Removing that unused feature removes the invalid
cross-crate AST boundary without downgrading the requested dependency updates
or changing the runtime binding generator.

The original `Cargo.toml.orig` remains unchanged as upstream evidence. The
normal locked persistence build is the regression gate; no build check is
skipped and no generated binding is substituted.
