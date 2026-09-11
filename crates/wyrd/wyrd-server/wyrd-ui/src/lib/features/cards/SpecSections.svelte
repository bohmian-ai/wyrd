<script lang="ts">
  // The shared typed Spec fallback: every Card kind without an earned
  // workspace renders its declaration as labeled key/value sections. A
  // section with nothing to show says so in place — never silently dropped.
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import type { SpecSection } from './core/types';

  let { sections, base }: { sections: SpecSection[]; base: string } = $props();
</script>

{#if !sections.length}
  <StateBlock
    state="absent"
    title="No Spec sections to show"
    detail="This Card's declaration carries no renderable sections at this version."
  />
{/if}
{#each sections as section (section.title)}
  <Panel title={section.title} variant="raised">
    {#snippet head()}{#if section.note}<span class="mono muted">{section.note}</span>{/if}{/snippet}
    {#if section.entries.length}
      <div class="kv">
        {#each section.entries as entry, i (i)}
          <span class="k">{entry.k}</span>
          {#if entry.href}<span class="mono"><a href={base + entry.href}>{entry.v}</a></span>
          {:else}<span class="mono">{entry.v}</span>{/if}
        {/each}
      </div>
    {/if}
    {#if section.absent}<p class="muted">{section.absent}</p>{/if}
  </Panel>
{/each}
