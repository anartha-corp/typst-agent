# typst-sync-upstream

Fetch `upstream` without a push credential, verify the target object and tags,
and update only `mirror/upstream-main` in a sync branch. Compare downstream
invariants and produce a compatibility/review pack. Never auto-merge, cherry-
pick into upstream, or push any ref to `typst/typst`.
