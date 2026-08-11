# typst-plan-change

Use before a cross-crate or multi-commit change. Emit a `TaskContract` with
scope, exclusions, invariant IDs, upstream anchor, and required checks. Split
independently reviewable slices on `execute-plan/typst-agent-native-pr-*`
branches. Do not widen scope because an agent found convenient adjacent code.
