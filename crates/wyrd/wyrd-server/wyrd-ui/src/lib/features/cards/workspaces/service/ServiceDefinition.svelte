<script lang="ts">
  // S-03 — the declaration-first Definition view (REQ-123): purpose and
  // entry point, runtime-aliased component refs, publication bindings split
  // by subject semantics, the closed raw-spec disclosure, and the composed
  // governance rail (principal/policy, server-managed metadata,
  // server-derived relationships, immutable versions). Secrets never render.
  import CodeBlock from '$lib/components/CodeBlock.svelte';
  import Disclosure from '$lib/components/Disclosure.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import type { CardDetail, ServicePresentation } from '../../core/types';
  import { serviceHref, type ServiceScope } from './service-state';

  let {
    spec,
    detail,
    base,
    scope,
    path
  }: {
    spec: ServicePresentation;
    detail: CardDetail;
    base: string;
    scope: ServiceScope;
    path: string;
  } = $props();

  const definition = $derived(spec.definition);
</script>

<div class="def-columns">
  <div class="stack">
    <Panel title={`DECLARATION — WHAT ${detail.version} DEFINES`} variant="quiet">
      {#snippet head()}<span class="mono muted">{definition.declaration.note}</span>{/snippet}
      <p>{definition.declaration.summary}</p>
      <div class="kv">
        {#each definition.declaration.entries as entry (entry.k)}
          <span class="k">{entry.k}</span><span class="mono">{entry.v}</span>
        {/each}
      </div>
    </Panel>
    <Panel title="COMPONENTS — RUNTIME-ALIASED CARD REFERENCES" variant="quiet">
      {#snippet head()}<span class="mono muted">{definition.components.note}</span>{/snippet}
      <Table label="Components">
        <table>
          <thead>
            <tr><th>Alias</th><th>Ref (Card · version)</th><th>Kind</th><th>publishes_to</th></tr>
          </thead>
          <tbody>
            {#each definition.components.rows as row, i (i)}
              <tr>
                <td class="mono alias">{row.alias}</td>
                <td class="mono"><a href={base + row.href}>{row.ref}</a></td>
                <td><span class="pill">● {row.kind.toUpperCase()}</span></td>
                {#if row.publishesHref}
                  <td class="mono"><a href={base + row.publishesHref}>{row.publishes}</a></td>
                {:else}
                  <td class="mono muted">{row.publishes}</td>
                {/if}
              </tr>
            {/each}
          </tbody>
        </table>
      </Table>
      <p class="mono muted">{definition.components.aliasNote}</p>
    </Panel>
    <Panel title="PUBLICATION BINDINGS — TWO SUBJECT SEMANTICS" variant="quiet">
      {#snippet head()}<span class="mono muted">{definition.publications.note}</span>{/snippet}
      <p class="mono muted">{definition.publications.component.label}</p>
      {#if definition.publications.component.flows.length}
        <p class="mono">
          {#each definition.publications.component.flows as flow, i (flow.from.label)}
            {#if i}<span class="muted"> · </span>{/if}
            <a href={base + flow.from.href}>{flow.from.label}</a> →
            <a href={base + flow.to.href}>{flow.to.label}</a>
          {/each}
        </p>
      {:else if definition.publications.component.absent}
        <StateBlock state="absent" title="NONE DECLARED" detail={definition.publications.component.absent} />
      {/if}
      <p class="mono muted">{definition.publications.service.label}</p>
      <StateBlock
        state="absent"
        title={definition.publications.service.value}
        detail={definition.publications.service.note}
      />
    </Panel>
    <Panel title="RAW SPEC — TYPED WIRE DISCLOSURE" variant="quiet">
      {#snippet head()}<span class="mono muted">{definition.rawSpec.meta}</span>{/snippet}
      <Disclosure summary={definition.rawSpec.summary}>
        <CodeBlock code={definition.rawSpec.yaml} />
      </Disclosure>
      <p class="mono muted">{definition.rawSpec.note}</p>
    </Panel>
  </div>
  <div class="stack">
    <Panel title="PRINCIPAL &amp; POLICY" variant="flat">
      {#snippet head()}<span class="mono muted">{definition.principal.note}</span>{/snippet}
      <div class="kv">
        {#each definition.principal.entries as entry, i (i)}
          <span class="k">{entry.k}</span>
          {#if entry.href}<span class="mono"><a href={base + entry.href}>{entry.v}</a></span>
          {:else if entry.k === 'principal'}
            <span><span class="principal-chip">{entry.v}</span></span>
          {:else}<span class="mono">{entry.v}</span>{/if}
        {/each}
      </div>
    </Panel>
    <Panel title="Metadata" variant="flat">
      {#snippet head()}<span class="mono muted">server-managed</span>{/snippet}
      <div class="kv">
        {#each detail.metadata as entry (entry.k)}
          <span class="k">{entry.k}</span><span class="mono">{entry.v}</span>
        {/each}
      </div>
    </Panel>
    <Panel title="Relationships" variant="flat">
      {#snippet head()}<span class="mono muted">server-derived · declaration ≠ execution</span>{/snippet}
      <div class="kv">
        {#each detail.relationships as rel (rel.relation + rel.label)}
          <span class="k">{rel.relation}</span>
          {#if rel.href}<span class="mono"><a href={base + rel.href}>{rel.label}</a></span>
          {:else}<span class="mono">{rel.label}</span>{/if}
        {/each}
      </div>
      <p class="mono muted">every ref keeps its own route: /cards/&lbrace;uid&rbrace;</p>
    </Panel>
    <Panel title="Versions" variant="flat">
      {#snippet head()}<span class="mono muted">immutable declarations</span>{/snippet}
      <div class="kv">
        {#each detail.versions as row (row.version)}
          <span class="k mono">
            <a
              href={serviceHref(path, row.version, scope, {})}
              aria-current={row.version === detail.version ? 'true' : undefined}
              >{row.version}{row.version === detail.currentVersion ? ' (current)' : ''}</a
            >
          </span>
          <span class="mono">registered {row.created} · {row.note}</span>
        {/each}
      </div>
    </Panel>
    <Panel title="SECRETS" variant="flat">
      <p class="muted">{definition.secrets}</p>
    </Panel>
  </div>
</div>
