# PDF, raster, and SVG output

Authority is the target encoder and its tests in `crates/typst-pdf/`,
`crates/typst-render/`, and `crates/typst-svg/`. Escape document data for the
target format and keep promised reproducibility. Reference updates require a
visual report and human approval.

Required checks are the target crate tests and `cargo agent verify --tier pr`.
