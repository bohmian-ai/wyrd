<!--
  One exact Repository Card version: the viewed version apart from the
  selected policy, the three separate facts, its exact Verifier bindings and
  the read-only declaration. Viewing a version never selects it, and nothing
  here edits, activates, pauses or rolls back policy.
-->
<script lang="ts">
  import type { ActionProps, CardRef } from '../contract.js';
  import { fmtAgo, fmtInstant, parseInstant } from '../format.js';
  import { ACCESS_WORDS, READINESS_WORDS, accessWhy, readinessWhy, type RepositoryPolicyProps } from '../repositories.js';
  import ActionNote from './ActionNote.svelte';
  import Code from './Code.svelte';
  import CopyButton from './CopyButton.svelte';
  import EmptyState from './EmptyState.svelte';
  import RepositorySheet from './RepositorySheet.svelte';

  let { input, result, actions = {}, onaction }: RepositoryPolicyProps & ActionProps = $props();

  const viewed = $derived(input.cardRef.version);
  const identity = $derived(`Repository/${input.cardRef.name}@${viewed}`);
  const declaration = $derived(input.declaration ?? 'yaml');
  const disabled = $derived(!onaction);
  const BINDING = { active: ['●', 'ok'], 'inactive · no access': ['⊘', 'warn'], 'not selected': ['○', 'muted'] } as const;
  const view = (version: string) => onaction?.({ name: 'card.version', input: { card_ref: { ...input.cardRef, version } } });
  const open = (ref: CardRef) => onaction?.({ name: 'card.open', input: { card_ref: ref } });
</script>

<RepositorySheet>
  {#if result.state === 'populated'}
    {@const data = result.data}
    {@const listed = data.versions.state === 'populated' ? data.versions.data.items : []}
    {@const current = listed.find((item) => item.version === viewed)}
    {@const host = (data.card.spec as { host: string }).host}
    <div class="xhead">
      <div class="xtitle">
        <div class="xname">
          <span class="kt">Repository</span>
          <h1 class="mono">{data.card.metadata.name}</h1>
          <select
            class="vsel"
            aria-label="Card version"
            disabled={disabled || data.versions.state !== 'populated' || actions['card.version']?.state === 'pending'}
            value={viewed}
            onchange={(event) => {
              view(event.currentTarget.value);
              event.currentTarget.value = viewed;
            }}
          >
            {#each listed as item (item.version)}<option value={item.version}>{item.version === data.selectedVersion ? `${item.version} · selected` : item.version}</option>{/each}
            {#if !current}<option value={viewed}>{viewed}</option>{/if}
          </select>
          {#if data.selected}<span class="badge c-ok">✓ selected</span>{:else}<span class="tag">not selected · viewing only</span>{/if}
        </div>
        <div class="xmeta">
          <span>{identity}</span>
          {#if data.card.metadata.uid}<span class="sep">·</span>uid {data.card.metadata.uid}{/if}
          {#if current}<span class="sep">·</span>registered <time datetime={current.created_at} title={fmtInstant(parseInstant(current.created_at))}>{fmtAgo(parseInstant(current.created_at))}</time>{/if}
          {#if data.versions.state !== 'populated'}<span class="sep">·</span>versions unavailable: {data.versions.problem.title} ({data.versions.problem.code}){/if}
        </div>
      </div>
      <div class="xactions">
        <CopyButton label="Copy identity" text={identity} />
        <CopyButton label="Copy link" text={() => location.href} />
        {#if data.fullName}
          <a class="btn btn-sm" href="https://{host}/{data.fullName}" target="_blank" rel="noopener noreferrer">{data.fullName} ↗</a>
        {:else}
          <span class="tag">repository_id {(data.card.spec as { repository_id: string }).repository_id}</span>
        {/if}
        <ActionNote state={actions['card.version'] ?? actions['card.open'] ?? actions['repository.declaration'] ?? actions['connection.open']} />
      </div>
    </div>
    {#if !data.selected && data.selectedVersion}
      {@const selected = data.selectedVersion}
      <div class="banner" role="status">
        You are viewing {viewed}. Automatic verification uses the selected version {selected}; viewing does not change it. To select {viewed} again, declare it with wyrd declare.
        <button type="button" class="link" {disabled} onclick={() => view(selected)}>View {selected}</button>
      </div>
    {/if}
    <dl class="facts">
      <div>
        <dt>Card</dt>
        <dd>
          <span class="st c-text"><span aria-hidden="true">●</span>Registered</span>
          <span class="why">{data.versions.state === 'populated' ? `${listed.length}${data.versions.data.truncated ? '+' : ''} version${listed.length === 1 ? '' : 's'} · ` : ''}{data.selectedVersion ? `selected ${data.selectedVersion}` : 'none selected'}</span>
        </dd>
      </div>
      <div>
        <dt>GitHub access</dt>
        <dd>
          <span class="st c-{ACCESS_WORDS[data.access].tone}"><span aria-hidden="true">{ACCESS_WORDS[data.access].glyph}</span>{ACCESS_WORDS[data.access].word}</span>
          <span class="why">{accessWhy(data.access, data.fullName, data.installation)}</span>
          <button type="button" class="link small" {disabled} onclick={() => onaction?.({ name: 'connection.open', input: {} })}>Manage connection</button>
        </dd>
      </div>
      <div>
        <dt>Automatic verification</dt>
        <dd>
          {#if data.readiness}
            {@const fact = READINESS_WORDS[data.readiness]}
            <span class="st c-{fact.tone}"><span aria-hidden="true">{fact.glyph}</span>{fact.word}</span>
            <span class="why">{readinessWhy(data.readiness, data.selectedBindings, data.selectedVersion)}</span>
          {:else}
            <span class="st c-muted"><span aria-hidden="true">○</span>Unavailable</span>
            <span class="why">The selected version {data.selectedVersion} could not be read.</span>
          {/if}
        </dd>
      </div>
    </dl>
    <div class="split">
      <section class="section" aria-labelledby="bindings-heading">
        <div class="section-head"><h2 class="h" id="bindings-heading">verified_by · {identity}</h2></div>
        {#if data.bindings.length}
          <div class="outs-wrap" role="region" aria-label="Verifier bindings" tabindex="-1">
            <table class="outs">
              <thead>
                <tr><th scope="col">Verifier</th><th scope="col">Exact version</th><th scope="col">Starts on</th><th scope="col">Binding</th><th scope="col">Definition</th></tr>
              </thead>
              <tbody>
                {#each data.bindings as binding, index (index)}
                  {@const ref = binding.ref}
                  <tr>
                    <td><div class="cellbox"><span class="mono">{binding.name}</span></div></td>
                    <td><div class="cellbox"><span class="mono">{binding.version ?? '—'}</span></div></td>
                    <td><div class="cellbox"><span class="mono">{binding.startsOn}</span></div></td>
                    <td><div class="cellbox"><span class="st c-{BINDING[binding.state][1]}"><span aria-hidden="true">{BINDING[binding.state][0]}</span>{binding.state}</span></div></td>
                    <td>
                      <div class="cellbox">
                        {#if ref}<button type="button" class="link" {disabled} title="Verifier/{ref.name}@{ref.version}" onclick={() => open(ref)}>Verifier definition</button>{:else}<span class="muted">not an exact Card</span>{/if}
                      </div>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <p class="note box">Version {viewed} declares verifier = []. Pull requests start no automatic verification; GitHub access is unaffected.</p>
        {/if}
      </section>
      <section class="panel" aria-label="Declaration (read-only)">
        <div class="tabs" role="tablist">
          {#each [['yaml', 'Card YAML'], ['toml', 'wyrd.toml']] as const as [key, label] (key)}
            <button type="button" class="tab" role="tab" aria-selected={declaration === key} {disabled} onclick={() => onaction?.({ name: 'repository.declaration', input: { declaration: key } })}>{label}</button>
          {/each}
        </div>
        <div class="tabpanel" role="tabpanel">
          <Code text={data.declaration} toml={declaration === 'toml'} />
          {#if declaration === 'toml'}<p class="note small">Reconstructed from the Card: a projection of its bindings, not the file that was declared.</p>{/if}
          <p class="note small">Read-only. Change policy by editing the declaration and running wyrd declare.</p>
        </div>
      </section>
    </div>
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading {input.cardRef.name}@{viewed}" />
  {:else if result.state !== 'empty'}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</RepositorySheet>
