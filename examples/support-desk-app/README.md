# support-desk app

The support-desk API that the docs quickstart walks through. It mirrors the
`wyrd-example-py`, `wyrd-example-rs`, and `wyrd-example-ts` developer
repositories, so the docs can embed the same code with `<CodeFromFile>`.

| Path | Holds |
| --- | --- |
| `service/` | The Cards: the `support-desk` Service, its Agent and Prompt, three Verifiers, and the refund policy PDF |
| `python/` | The FastAPI app, deploy script, and evidence query |
| `rust/lib.rs` | The axum app, from `wyrd-example-rs` |
| `typescript/` | The Node app, deploy script, and evidence query, from `wyrd-example-ts` |

`ANCHOR: name` / `ANCHOR_END: name` comments mark the regions the docs embed.
Keep the three languages in step: same Service, same `tickets` table, same
`/ask` and `/reviews` routes.

Only the Python app runs from this directory. The Rust and TypeScript copies
are reference code; their full projects, with manifests and tests, live in the
example repositories.
