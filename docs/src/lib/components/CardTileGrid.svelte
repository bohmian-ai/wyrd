<script lang="ts">
  import { base } from '$app/paths';

  // The card-kind catalog, rendered as the Direction A `.grid3` of `.card`
  // tiles. Status mirrors the hand-authored table in content/docs/cards/index:
  // Data / Model / Prompt ship end to end; every other kind is specified (schema
  // + validation) but has no holder/runnable surface yet. Shipped kinds link to
  // their card page; spec-only kinds link to the generated Schema reference.
  // `lane` maps each kind to its cabinet color (client / server / control /
  // rune) — the same lane the kbar + dot carry across Direction A.
  type Status = 'shipped' | 'spec';
  type Lane = 'client' | 'server' | 'control' | 'rune';
  type Tile = { slug: string; label: string; lane: Lane; status: Status };

  const tiles: Tile[] = [
    { slug: 'data', label: 'Data', lane: 'client', status: 'shipped' },
    { slug: 'model', label: 'Model', lane: 'rune', status: 'shipped' },
    { slug: 'prompt', label: 'Prompt', lane: 'rune', status: 'shipped' },
    { slug: 'agent', label: 'Agent', lane: 'client', status: 'spec' },
    { slug: 'workflow', label: 'Workflow', lane: 'client', status: 'spec' },
    { slug: 'artifact', label: 'Artifact', lane: 'rune', status: 'spec' },
    { slug: 'eval', label: 'Eval', lane: 'server', status: 'spec' },
    { slug: 'experiment', label: 'Experiment', lane: 'server', status: 'spec' },
    { slug: 'drift', label: 'Drift', lane: 'rune', status: 'spec' },
    { slug: 'policy', label: 'Policy', lane: 'control', status: 'spec' },
    { slug: 'audit', label: 'Audit', lane: 'control', status: 'spec' },
    { slug: 'service', label: 'Service', lane: 'server', status: 'spec' },
    { slug: 'operator', label: 'Operator', lane: 'control', status: 'spec' },
    { slug: 'source', label: 'Source', lane: 'client', status: 'spec' },
    { slug: 'trigger', label: 'Trigger', lane: 'client', status: 'spec' },
    { slug: 'mcp', label: 'MCP', lane: 'server', status: 'spec' }
  ];

  function href(t: Tile): string {
    return t.status === 'shipped' ? `${base}/cards/${t.slug}/` : `${base}/api/schemas/`;
  }
</script>

<div class="grid3 not-content">
  {#each tiles as t (t.slug)}
    <a class="card" href={href(t)}>
      <div class="kbar {t.lane}"></div>
      <div class="ct"><span class="d {t.lane}"></span>{t.label}</div>
      <div class="meta">{t.status === 'shipped' ? 'Shipped →' : 'Spec only →'}</div>
    </a>
  {/each}
</div>
