# Deterministic agent evaluation harness

The strict tasks in `evals/tasks/` are executed in isolated disposable
worktrees backed by per-scenario bare clones. Each task declares a bounded
scope, typed operations, and deterministic graders. Operations can apply only
reviewed patches from `evals/fixtures/`, write one of the built-in hostile
fixtures, create a signed fixture commit, seed the isolated mirror ref, or run
a fixed `cargo agent` command. Task files cannot execute arbitrary shell.

`cargo agent eval` runs all ten scenarios and verifies command exit codes, JSON
fields, scope containment, secret redaction, reference rejection, reverse Cargo
dependencies, seeded review evidence, and the inert upstream boundary. Run it
directly or through `scripts/agent-eval.sh`. The harness never mutates the
caller worktree or its refs, and removes each scenario sandbox on exit. A model
evaluation may be attached as optional evidence but is never a correctness
dependency.
