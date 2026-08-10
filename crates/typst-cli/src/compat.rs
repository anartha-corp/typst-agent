// Keep an upstream-compatible `typst` executable for integrations that still
// invoke that name. The published downstream artifact and container are built
// from the `typst-agent` target.
include!("main.rs");
