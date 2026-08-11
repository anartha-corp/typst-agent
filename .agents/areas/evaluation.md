# Evaluation and World

Authority is the `World` implementation in `crates/typst/src/` and evaluation
code in `crates/typst-eval/` and `crates/typst-library/`. External resources must
flow through the explicit World; do not add hidden network, clock, randomness,
or process-global state. Keep evaluation deterministic and cache-safe.

Required checks: `cargo test -p typst-eval -p typst-library`, plus the relevant
agent verification tier. Review `eval-pure-world` and `eval-deterministic`.

The agent attack harness under `evals/` is a separate model-free evaluation
surface. Its TOML tasks are strict data, not shell programs. Each scenario must
run in an isolated disposable worktree, enforce declared write scope, grade
actual command JSON and exit codes, and clean its sandbox even after failure.
`cargo agent eval` and `cargo test -p typst-agent-dev` are authoritative for
that surface; model evaluation is optional evidence only.
