# Agent navigation index

Start with `cargo agent doctor`, then choose the narrowest guide matching the
changed paths. These guides are navigation aids; source, tests, Cargo metadata,
and ordered upstream history remain authoritative.

| Changed surface | Guide | Invariants |
| --- | --- | --- |
| `crates/typst-syntax/` | [parser and spans](areas/parser-spans.md) | `syntax-parse-total`, `syntax-span-stable` |
| `crates/typst-eval/`, `crates/typst-library/` | [evaluation](areas/evaluation.md) | `eval-pure-world`, `eval-deterministic` |
| `crates/typst/`, `crates/typst-layout/`, `crates/typst-realize/` | [layout](areas/layout.md) | `layout-introspection`, `layout-incremental` |
| `crates/typst-ide/` | [IDE](areas/ide.md) | `ide-span-contract` |
| `crates/typst-cli/` | [CLI](areas/cli.md) | `cli-no-network-default`, `cli-agent-name` |
| `crates/typst-pdf/`, `crates/typst-render/`, `crates/typst-svg/` | [output](areas/output.md) | `output-escape`, `output-reproducible` |
| `tests/` | [tests](areas/tests.md) | `tests-reference-review` |
| `.github/`, `Dockerfile`, release scripts | [release](areas/release.md) | `release-provenance`, `release-human-gate` |

Cross-cutting rules live in [`AGENTS.md`](../AGENTS.md), the versioned contract
in [`agent-contract/v1/`](../agent-contract/v1/), and the invariant registry in
[`invariants.yml`](invariants.yml).
