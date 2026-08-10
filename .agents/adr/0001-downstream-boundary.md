# ADR 0001: Keep the downstream boundary explicit

Status: accepted

## Decision

Typst Agent is a compiler downstream, not an agent runtime. Development
automation lives in the non-publishable `typst-agent-dev` crate, writes only
`.tmp/agent/`, and cannot push to upstream or make merge/release decisions.

## Consequences

The compiler stays portable and deterministic. Contributors get bounded context,
impact, and evidence commands. Human review remains a required authority and
upstream synchronization can be audited from Git objects.
