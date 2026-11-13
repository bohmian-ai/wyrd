# wyrd-ai

TypeScript SDK for [Wyrd](https://github.com/bohmian-ai/wyrd), the real-time
verification system built for your applications. Before your application acts,
it asks Wyrd whether the action meets your requirements, and acts on the
verdict. [Run your first verification](https://docs.bohmian.ai/wyrd/get-started/quickstart/)
shows it end to end.

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

// Buffer a row; shutdown drains it
await run.bifrost.writeTo("bifrost.datasets.answers", { question, answer });

// Run a Verifier now and wait for the verdict
const result = await run.observe.verify("my-check", { answer });
console.log(result.outcome.verdict);

await state.shutdown(); // flush everything recorded
```

See the [repository](https://github.com/bohmian-ai/wyrd) for setup, Cards,
Verifiers, and the server.

## License

Apache-2.0
