# Evaluation and World

Authority is the `World` implementation in `crates/typst/src/` and evaluation
code in `crates/typst-eval/` and `crates/typst-library/`. External resources must
flow through the explicit World; do not add hidden network, clock, randomness,
or process-global state. Keep evaluation deterministic and cache-safe.

Required checks: `cargo test -p typst-eval -p typst-library`, plus the relevant
agent verification tier. Review `eval-pure-world` and `eval-deterministic`.
