import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { DIAGRAMS } from './index';
import { DISPATCH_STATES, RUN_STATES, VERDICTS, runLifecycle } from './verification';

// The run state machine must name exactly the states the server reports.
function schemaEnum(file: string, definition: string): string[] {
  const schema = JSON.parse(readFileSync(new URL(`../../../../crates/wyrd-spec/schemas/${file}`, import.meta.url), 'utf8'));
  const def = schema.definitions[definition];
  return (def.oneOf ?? [def]).flatMap((v: { enum: string[] }) => v.enum).sort();
}

describe('run lifecycle diagram', () => {
  it('matches the server status enums', () => {
    expect([...RUN_STATES].sort()).toEqual(schemaEnum('verification_run_status.json', 'VerificationExecutionStatus'));
    expect([...DISPATCH_STATES].sort()).toEqual(schemaEnum('verification_run_status.json', 'OperatorDispatchStatus'));
    expect([...VERDICTS].sort()).toEqual(schemaEnum('operator_failure_context.json', 'VerificationVerdict'));
  });

  it('draws a state for every status', () => {
    const titles = runLifecycle.views[0].nodes.filter((n) => n.kind === 'state').map((n) => n.title);
    for (const s of [...RUN_STATES, ...VERDICTS, ...DISPATCH_STATES]) expect(titles).toContain(s);
  });
});

describe('diagram data', () => {
  for (const [name, spec] of Object.entries(DIAGRAMS)) {
    if (spec.type !== 'diagram') continue;
    it(`${name} references only ids it draws`, () => {
      const nodes = new Set(spec.views.flatMap((v) => v.nodes.map((n) => n.id)));
      const edges = new Map(spec.views.flatMap((v) => v.edges.map((e) => [e.id, e] as const)));
      for (const e of edges.values()) {
        if (e.from) expect(nodes).toContain(e.from);
        if (e.to) expect(nodes).toContain(e.to);
      }
      for (const s of spec.steps ?? []) {
        s.nodes.forEach((id) => expect(nodes).toContain(id));
        s.edges.forEach((id) => expect(edges.has(id)).toBe(true));
      }
      for (const sc of spec.scenarios ?? []) {
        expect(sc.say).toHaveLength(sc.path.length);
        sc.path.slice(1).forEach((to, i) => {
          const from = sc.path[i];
          expect([...edges.values()].some((e) => e.from === from && e.to === to), `${from} → ${to}`).toBe(true);
        });
      }
    });
  }
});
