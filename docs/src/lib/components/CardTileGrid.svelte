<script lang="ts">
  import { base } from '$app/paths';

  // The card-kind catalog. Status mirrors the hand-authored table in
  // content/docs/cards/index.mdx: Data / Model / Prompt ship end to end; every
  // other kind is specified (schema + validation) but has no holder/runnable
  // surface yet. Shipped kinds link to their card page; spec-only kinds link to
  // the generated Schema reference where their fields are documented.
  type Status = 'shipped' | 'spec';
  type Tile = { slug: string; label: string; kind: string; status: Status };

  const tiles: Tile[] = [
    { slug: 'data', label: 'Data', kind: 'data', status: 'shipped' },
    { slug: 'model', label: 'Model', kind: 'model', status: 'shipped' },
    { slug: 'prompt', label: 'Prompt', kind: 'prompt', status: 'shipped' },
    { slug: 'agent', label: 'Agent', kind: 'agent', status: 'spec' },
    { slug: 'workflow', label: 'Workflow', kind: 'workflow', status: 'spec' },
    { slug: 'artifact', label: 'Artifact', kind: 'artifact', status: 'spec' },
    { slug: 'eval', label: 'Eval', kind: 'eval', status: 'spec' },
    { slug: 'experiment', label: 'Experiment', kind: 'experiment', status: 'spec' },
    { slug: 'drift', label: 'Drift', kind: 'drift', status: 'spec' },
    { slug: 'policy', label: 'Policy', kind: 'policy', status: 'spec' },
    { slug: 'audit', label: 'Audit', kind: 'audit', status: 'spec' },
    { slug: 'service', label: 'Service', kind: 'service', status: 'spec' },
    { slug: 'operator', label: 'Operator', kind: 'operator', status: 'spec' },
    { slug: 'source', label: 'Source', kind: 'source', status: 'spec' },
    { slug: 'trigger', label: 'Trigger', kind: 'trigger', status: 'spec' },
    { slug: 'mcp', label: 'MCP', kind: 'mcp', status: 'spec' }
  ];

  function href(t: Tile): string {
    return t.status === 'shipped' ? `${base}/cards/${t.slug}/` : `${base}/api/schemas/`;
  }
</script>

<ul class="wyrd-tile-grid not-content">
  {#each tiles as t (t.slug)}
    <li>
      <a href={href(t)} data-kind={t.kind}>
        {t.label}
        <small>{t.status === 'shipped' ? 'shipped' : 'spec only'}</small>
      </a>
    </li>
  {/each}
</ul>
