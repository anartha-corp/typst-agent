# IDE integration

Authority is `crates/typst-ide/src/` and its tests. Completion, navigation, and
edits must use syntax-owned spans and validate against the current source.
Changes that alter parser spans require a cross-crate impact report.

Required checks: `cargo test -p typst-ide` and review of `ide-span-contract`.
