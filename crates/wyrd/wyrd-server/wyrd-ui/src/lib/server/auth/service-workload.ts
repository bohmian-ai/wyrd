/**
 * The shared live Service workload for the UI journeys: `count` Services, each
 * registered, credentialed and exercised only through public pathways — Card
 * registration, the Service's own Card-bound credential (whose exchange
 * activates its bindings), OTLP root spans carrying the Service's
 * `service.name`/`service.version`, Eval and Drift observations, a manual
 * Drift run, and a bounded readiness wait on the retained evidence.
 *
 * Services share one Prompt; each has its own Agent so a Service's Eval
 * results come only from its own observations. Even-indexed Services pass both
 * Verifiers and odd-indexed ones fail them. Every Service also emits one root
 * span three hours back, outside the `1h` range and inside `6h`.
 */
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { SpanStatusCode } from '@opentelemetry/api';
import { resourceFromAttributes } from '@opentelemetry/resources';
import { BasicTracerProvider, BatchSpanProcessor } from '@opentelemetry/sdk-trace-base';
import { Bifrost, Cards, Principals, WyrdClient, WyrdState, type CardRef } from '@wyrd/sdk';
import { spanExporter } from '@wyrd/sdk/otel';

/** One exercised Service and the evidence it is expected to show. */
export type WorkloadService = {
  name: string;
  version: string;
  uid: string;
  agentUid: string;
  evalUid: string;
  driftUid: string;
  /** Whether its Eval and Drift results pass. */
  passing: boolean;
  /** Root spans inside the last hour, how many of them errored, and the older one. */
  recentSpans: number;
  recentErrors: number;
  /** Eval observations, each judged once by the continuous binding. */
  evaluations: number;
};

/** Root spans per Service within the hour; odd Services add errors and more volume. */
const spansFor = (i: number) => 4 + 2 * (i % 2);
const errorsFor = (i: number) => (i % 2 ? 2 : 0);
/** How long evidence may take to become queryable. */
const READY_MS = 180_000;

const card = (kind: string, name: string, spec: string) =>
  `apiVersion: wyrd/v1\nkind: ${kind}\nmetadata:\n  name: ${name}\n  version: 1.0.0\n  space: default\nspec:\n${spec}`;

/**
 * Register, credential and exercise `count` Services named `<prefix>-svc-<i>`
 * as `admin`, and wait until every Service's spans and results are queryable.
 *
 * @throws when a registration, emission or run fails, or evidence is not visible within the bound.
 */
export async function serviceWorkload(admin: WyrdClient, count = 2, prefix = 'wl'): Promise<WorkloadService[]> {
  const dir = await mkdtemp(join(tmpdir(), 'wyrd-workload-'));
  try {
    const promptName = `${prefix}-prompt`;
    const evalName = `${prefix}-answer`;
    const driftName = `${prefix}-score-drift`;
    await writeFile(
      join(dir, 'prompt.yaml'),
      card('Prompt', promptName, '  provider: openai\n  model: gpt-4o\n  messages: [answer the customer]\n')
    );
    await writeFile(
      join(dir, 'eval.yaml'),
      card(
        'Verifier',
        evalName,
        '  implementation:\n    kind: eval\n    spec:\n      pass_gate: {kind: all_pass}\n      tasks:\n        answer:\n          kind: assertion\n          id: answer\n          context_path: $.answer\n          operator: equals\n          expected: yes\n'
      )
    );
    await writeFile(
      join(dir, 'drift.yaml'),
      card(
        'Verifier',
        driftName,
        '  implementation:\n    kind: drift\n    spec:\n      method: Custom\n      signal:\n        kind: Metric\n        name: score\n      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n'
      )
    );
    const cards = Cards.connect({ client: admin });
    const services: WorkloadService[] = [];
    const roots: CardRef[] = [];
    // Registration is sequential: the shared Prompt and Verifiers register once, then are reused.
    for (let i = 0; i < count; i++) {
      const name = `${prefix}-svc-${i}`;
      await writeFile(
        join(dir, `agent-${i}.yaml`),
        card('Agent', `${prefix}-agent-${i}`, `  prompt: ./prompt.yaml\n  run_config:\n    max_iterations: 1\n`)
      );
      await writeFile(
        join(dir, `svc-${i}.yaml`),
        card(
          'Service',
          name,
          `  service_type: agent\n  description: Workload Service ${i}.\n  components:\n    - alias: agent\n      ref: ./agent-${i}.yaml\n      verified_by:\n        - verifier: ./eval.yaml\n          runs_on: {kind: observations_ready}\n    - alias: prompt\n      ref: ./prompt.yaml\n  verified_by:\n    - verifier: ./drift.yaml\n      runs_on: {kind: schedule, cron: "0 2 * * *"}\n`
        )
      );
      const receipt = await cards.registerFromPath(join(dir, `svc-${i}.yaml`));
      const uidOf = (n: string) => {
        const uid = receipt.outcomes.find((o) => o.card_ref.name === n)?.card_ref.uid;
        if (!uid) throw new Error(`registration of ${name} returned no uid for ${n}`);
        return uid;
      };
      roots.push(receipt.root);
      services.push({
        name,
        version: '1.0.0',
        uid: uidOf(name),
        agentUid: uidOf(`${prefix}-agent-${i}`),
        evalUid: uidOf(evalName),
        driftUid: uidOf(driftName),
        passing: i % 2 === 0,
        recentSpans: spansFor(i),
        recentErrors: errorsFor(i),
        evaluations: spansFor(i)
      });
    }
    const principals = await servicePrincipals(admin);
    await Promise.all(
      services.map((service, i) =>
        exercise(admin, cards, roots[i], principals.get(service.uid), service, i, join(dir, `bundle-${i}`))
      )
    );
    await ready(admin, services);
    return services;
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
}

/** Authenticated `/v1` JSON call as `client`; throws on a non-2xx answer. */
async function v1(client: WyrdClient, method: string, path: string, body?: unknown): Promise<unknown> {
  const response = await fetch(`${client.serverUrl}/v1${path}`, {
    method,
    headers: {
      'x-wyrd-access-token': `Bearer ${await client.accessToken()}`,
      ...(body === undefined ? {} : { 'content-type': 'application/json' })
    },
    body: body === undefined ? undefined : JSON.stringify(body)
  });
  const text = await response.text();
  if (!response.ok) throw new Error(`${method} ${path} → ${response.status}: ${text}`);
  return text ? JSON.parse(text) : undefined;
}

/** Every Card-bound Service principal, keyed by its Service Card uid. */
async function servicePrincipals(admin: WyrdClient): Promise<Map<string, string>> {
  const principals = Principals.connect({ client: admin });
  const byCard = new Map<string, string>();
  for (let after: string | undefined; ; ) {
    const page = await principals.list({ kind: 'service', limit: 200, after });
    for (const p of page.principals) if (p.card_ref?.uid) byCard.set(p.card_ref.uid, p.principal_id);
    if (!page.next) return byCard;
    after = page.next;
  }
}

/**
 * Credential the Service's own principal, emit its spans and observations as
 * that principal, and run its Drift binding over the emitted window.
 */
async function exercise(
  admin: WyrdClient,
  cards: Cards,
  root: CardRef,
  principalId: string | undefined,
  service: WorkloadService,
  i: number,
  bundle: string
) {
  if (!principalId) throw new Error(`no Card-bound principal for ${service.name}`);
  const started = Date.now();
  const issued = await Principals.connect({ client: admin }).issueCredential(principalId);
  // The Service's own exchange is what makes its bindings runtime-active.
  const own = WyrdClient.connect({ serverUrl: admin.serverUrl, credential: issued.credential, grpcUrl: admin.grpcUrl });
  await own.accessToken();
  await cards.hydrate(root, bundle);
  const state = WyrdState.fromPath(bundle, { client: own });
  await state.startBifrost();
  const provider = new BasicTracerProvider({
    resource: resourceFromAttributes({ 'service.name': service.name, 'service.version': service.version }),
    spanProcessors: [new BatchSpanProcessor(spanExporter(own))]
  });
  const tracer = provider.getTracer('wyrd-ui-workload');
  const now = Date.now();
  const span = (at: number, durationMs: number, failed: boolean) => {
    const s = tracer.startSpan(`${service.name}.request`, { startTime: at });
    if (failed) s.setStatus({ code: SpanStatusCode.ERROR });
    s.end(at + durationMs);
    return s.spanContext();
  };
  span(now - 3 * 3_600_000, 500, false);
  for (let k = 0; k < service.recentSpans; k++) {
    const context = span(now - (k + 1) * 60_000, 100 + 50 * k, k < service.recentErrors);
    state
      .run('agent')
      .observe.eval({ answer: service.passing ? 'yes' : 'no' }, { traceId: context.traceId, spanId: context.spanId });
    state.run().observe.drift({ score: service.passing ? 1.1 : 2.0 + (i % 3) * 0.1 });
  }
  await provider.shutdown();
  await state.shutdown();
  // The Drift binding judges this Service's own observations over the emitted window.
  const { binding_ids = [] } = (await cards.get(root)).status?.verification ?? {};
  for (const binding_id of binding_ids) {
    const status = (await v1(admin, 'GET', `/verification/bindings/${binding_id}`)) as { verifier_uid: string };
    if (status.verifier_uid !== service.driftUid) continue;
    await v1(admin, 'POST', '/verification/runs', {
      target: { kind: 'binding', binding_id },
      input: { kind: 'drift_window', start: new Date(started - 2 * 3_600_000).toISOString(), end: new Date().toISOString() }
    });
  }
}

/** Wait, bounded, until every Service's recent spans and both Verifiers' results are queryable. */
async function ready(admin: WyrdClient, services: WorkloadService[]) {
  const bifrost = await Bifrost.connect({ client: admin });
  const deadline = Date.now() + READY_MS;
  const rows = async <T>(sql: string, params: (string | number)[]) =>
    (await bifrost.sql(sql, params)).toArrow().toArray().map((row) => row.toJSON() as T);
  try {
    for (;;) {
      const missing: string[] = [];
      for (const s of services) {
        const [spans] = await rows<{ n: bigint }>(
          'SELECT COUNT(*) AS n FROM vala.traces.spans WHERE service_name = $1 AND service_version = $2 AND parent_span_id IS NULL',
          [s.name, s.version]
        );
        const results = await rows<{ implementation: string; n: bigint }>(
          'SELECT implementation, COUNT(*) AS n FROM vala.verification.results WHERE owner_card_uid = $1 GROUP BY implementation',
          [s.uid]
        );
        const count = (kind: string) => Number(results.find((r) => r.implementation === kind)?.n ?? 0);
        if (Number(spans?.n ?? 0) < s.recentSpans + 1 || count('eval') < s.evaluations || count('drift') < 1)
          missing.push(`${s.name}: spans ${spans?.n ?? 0} eval ${count('eval')} drift ${count('drift')}`);
      }
      if (!missing.length) return;
      if (Date.now() >= deadline) throw new Error(`workload evidence incomplete: ${missing.join('; ')}`);
      await sleep(1_000);
    }
  } finally {
    await bifrost.shutdown();
  }
}
