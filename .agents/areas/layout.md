# Layout, realization, and introspection

Authority is `crates/typst-layout/`, `crates/typst-realize/`, and their callers
in `crates/typst/`. Layout and introspection must derive from one realized
document and preserve invalidation edges. Never add a second semantic renderer
to satisfy a snapshot.

Required checks: `cargo test -p typst-layout -p typst-realize -p typst`, and
review of `layout-introspection` and `layout-incremental`.
