# Tests and references

Authority is `tests/src/tests.rs`, test fixtures, and the crate tests. Prefer a
focused test first, then the applicable workspace lane. Reference images and
hashes are not disposable fixtures: changing one requires a review pack,
invariant impact, and a human approval marker.

The agent kernel refuses dirty-worktree scope escapes and baseline changes that
do not carry evidence.
