import { loadCardHeader, loadChart, resolveRange } from 'wyrd-ai-ui/loaders';
import { executor } from '../executor';
import { range, service, spans } from '../panels';

/** Load each component's props on the server; the browser receives props only. */
export async function load() {
  const context = { executor: executor(), range: resolveRange(range) };
  const [header, chart] = await Promise.all([loadCardHeader(service, context), loadChart(spans, context)]);
  return { header, chart };
}
