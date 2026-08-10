# Deterministic agent evaluation harness

The tasks in `evals/tasks/` are disposable-worktree scenarios for the
model-free control plane. Each task names a bounded scope, an expected policy
signal, and a deterministic grader. A model run may be attached as evidence but
is never required for correctness.

Run the catalog check with `cargo agent eval` and the disposable-worktree
rehearsal with `scripts/agent-eval.sh`. The harness never writes the working
tree or upstream; temporary worktrees are removed on exit.
