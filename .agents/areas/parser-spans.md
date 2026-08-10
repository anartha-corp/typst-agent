# Parser and spans

Authority is `crates/typst-syntax/src/` and its tests. Read the lexer, syntax
tree, span, and recovery code before changing parsing. Preserve total recovery,
source offsets, token ordering, and diagnostics for malformed documents.

Required checks: `cargo test -p typst-syntax`, `cargo fmt --check`, and the
diff-aware `cargo agent verify --tier pr`. Review `syntax-parse-total` and
`syntax-span-stable`; compare any changed reference output with the upstream
anchor in `.agents/invariants.yml`.
