# wyrd-ai

TypeScript SDK for [Wyrd](https://github.com/bohmian-ai/wyrd), open-source
verification and assurance for AI systems. Record what your agents do, verify
it against the Verifiers you registered, and query the evidence.

```bash
pnpm add wyrd-ai
```

The package ships a native addon. pnpm installs the one for your platform:
macOS on Apple Silicon, or Linux x64 or arm64 with glibc.

## Usage

```typescript
import { WyrdState } from "wyrd-ai";

const state = WyrdState.fromPath(".wyrd-bundle");
await state.startBifrost();

const run = state.run();

// Store a row
await run.observe.record("bifrost.datasets.answers", { question, answer });

// Run a Verifier now and wait for the verdict
const result = await run.observe.verify("my-check", { answer });
console.log(result.outcome.verdict);

await state.shutdown(); // flush everything recorded
```

See the [repository](https://github.com/bohmian-ai/wyrd) for setup, Cards,
Verifiers, and the server.

## License

Apache-2.0
