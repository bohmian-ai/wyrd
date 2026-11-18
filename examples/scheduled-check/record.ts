// Record five checkout requests that each took the given number of milliseconds.
import { WyrdState } from "wyrd-ai";

// The checkout Service, pulled at deploy time.
const state = WyrdState.fromPath(".wyrd-bundle");

const latency = Number(process.argv[2]);
const run = state.run();
for (let request = 0; request < 5; request++) {
  await run.observe.submit("latency-budget", { latency });
}
await state.shutdown(); // send the samples before the program exits
console.log(`recorded 5 requests at ${latency} ms`);
