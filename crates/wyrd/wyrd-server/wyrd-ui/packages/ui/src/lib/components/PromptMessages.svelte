<!--
  An exact Prompt as authored, which is exactly what the provider receives:
  provider, model and response format, then portable system text and
  numbered turns, or the native format's body unchanged, extension fields
  included.
-->
<script lang="ts">
  import type { PromptMessagesProps } from '../cards.js';
  import { fmtYaml } from '../format.js';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result }: PromptMessagesProps = $props();

  const spec = $derived(result.state === 'populated' ? result.data : undefined);
  const responseFormat = $derived(
    spec?.response_format == null ? undefined : typeof spec.response_format === 'string' ? spec.response_format : JSON.stringify(spec.response_format)
  );
  /** One content block as text; media keeps its literal source or `${media:name}` binding. */
  const block = (part: { type: string; text?: string; source?: unknown }) =>
    part.type === 'text' ? (part.text ?? '') : `[${part.type}] ${typeof part.source === 'string' ? part.source : JSON.stringify(part.source)}`;
</script>

{#if spec}
  <Panel title="Prompt" note="exactly what the provider receives">
    <p class="chips">
      <span>provider <b>{spec.provider}</b></span>
      <span>model <b>{spec.model}</b></span>
      {#if spec.format}<span>native <b>{spec.format}</b></span>{/if}
      <span>response <b>{responseFormat ?? 'text'}</b></span>
    </p>
    {#if spec.body}
      <section aria-label="Native body">
        <h4>Native body · unchanged</h4>
        <pre>{fmtYaml(spec.body)}</pre>
      </section>
    {:else}
      {#if spec.system}
        <section aria-label="System">
          <h4>System</h4>
          <pre>{spec.system}</pre>
        </section>
      {:else}
        <p class="muted">No system message.</p>
      {/if}
      {#each spec.messages ?? [] as message, index (index)}
        <section aria-label="Message {index + 1}">
          <h4>{index + 1} · {message.role}</h4>
          <pre>{typeof message.content === 'string' ? message.content : message.content.map(block).join('\n\n')}</pre>
        </section>
      {/each}
    {/if}
    {#if spec.settings && Object.keys(spec.settings).length > 0}
      <section aria-label="Settings">
        <h4>Settings</h4>
        <pre>{fmtYaml(spec.settings)}</pre>
      </section>
    {/if}
  </Panel>
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {input.cardRef.name}@{input.cardRef.version}" />
{:else if result.state === 'denied' || result.state === 'error'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  .chips span {
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 2px 8px;
    color: var(--muted);
  }
  b {
    color: var(--text);
    font-weight: 400;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h4 {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  p {
    margin: 0;
    font: 12px var(--font-mono);
  }
</style>
