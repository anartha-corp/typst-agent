# AI-assisted development disclosure

Typst Agent permits AI-assisted implementation, planning, testing, and review.
The compiler/runtime itself has no agent, MCP, networked model, or LLM dependency.

Every pull request that used AI must include a short disclosure describing:

1. which work was AI-assisted (planning, code, tests, review, or documentation);
2. the human who reviewed the complete diff and owns the result; and
3. the deterministic commands and evidence used to validate it.

AI output is untrusted input. Generated code must be inspected for scope escape,
secret exposure, reference-baseline laundering, and accidental upstream
publication. Model runs may be attached as evaluation evidence, but are never a
required correctness test or a substitute for human approval.
