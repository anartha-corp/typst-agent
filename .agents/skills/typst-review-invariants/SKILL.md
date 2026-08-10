# typst-review-invariants

Review the changed paths against `.agents/invariants.yml`. For each record,
answer its review prompts, inspect source and tests, and include an
`InvariantRecord` impact in the review pack. Reference-image/hash changes need
a visual report and explicit human approval; baseline updates are never an
automatic test fix.
