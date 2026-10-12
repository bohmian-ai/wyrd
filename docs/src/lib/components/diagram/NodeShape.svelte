<svelte:options namespace="svg" />

<script lang="ts">
  // Draws one diagram node by kind: the shape, a halo for selection, and its
  // labels. The caller wraps it in a <g> that owns classes and events.
  import type { DiagramNode } from '$lib/diagrams/types';

  let { node: n }: { node: DiagramNode } = $props();

  const cx = $derived(n.x + n.w / 2);
  const ink = $derived(n.kind === 'evidence' ? ' wd-t-ink' : '');
  const fill = $derived(
    {
      component: 'wd-f-surface wd-s-border',
      client: 'wd-f-surface2 wd-s-border wd-s-dash',
      declare: 'wd-f-declare wd-s-declare',
      evidence: 'wd-f-evidence',
      state: `wd-f-surface wd-tone-${n.tone}`
    }[n.kind as string] ?? 'wd-f-surface wd-s-border'
  );
  const ry = 9;
</script>

{#if n.kind === 'store'}
  <path
    d={`M${n.x} ${n.y + ry} a${n.w / 2} ${ry} 0 0 1 ${n.w} 0 v${n.h - 2 * ry} a${n.w / 2} ${ry} 0 0 1 ${-n.w} 0 z`}
    class="wd-f-surface2 wd-s-border"
  />
  <path d={`M${n.x} ${n.y + ry} a${n.w / 2} ${ry} 0 0 0 ${n.w} 0`} class="wd-s-border" fill="none" />
  <path
    d={`M${n.x - 3} ${n.y + ry} a${n.w / 2 + 3} ${ry + 3} 0 0 1 ${n.w + 6} 0 v${n.h - 2 * ry} a${n.w / 2 + 3} ${ry + 3} 0 0 1 ${-n.w - 6} 0 z`}
    class="wd-halo"
  />
{:else if n.kind === 'decision'}
  <polygon
    points={`${cx},${n.y} ${n.x + n.w},${n.y + n.h / 2} ${cx},${n.y + n.h} ${n.x},${n.y + n.h / 2}`}
    class="wd-f-surface2 wd-s-border"
  />
  <polygon
    points={`${cx},${n.y - 5} ${n.x + n.w + 6},${n.y + n.h / 2} ${cx},${n.y + n.h + 5} ${n.x - 6},${n.y + n.h / 2}`}
    class="wd-halo"
  />
{:else if n.kind === 'table'}
  {@const cols = n.cols ?? []}
  <rect x={n.x} y={n.y} width={n.w} height={26 + cols.length * 20 + 6} rx="2" class="wd-f-surface wd-s-border" />
  <rect x={n.x} y={n.y} width={n.w} height="26" rx="2" class="wd-f-surface2 wd-s-border" />
  <text x={n.x + 10} y={n.y + 17} class="wd-t-code">{n.title}</text>
  {#each cols as c, i (c.name)}
    <text x={n.x + 10} y={n.y + 41 + 20 * i} class="wd-t-colk">{c.key ? '◆' : c.fk ? '→' : '·'}</text>
    <text x={n.x + 26} y={n.y + 41 + 20 * i} class="wd-t-col">{c.name}</text>
  {/each}
  <rect x={n.x - 3} y={n.y - 3} width={n.w + 6} height={26 + cols.length * 20 + 12} rx="3" class="wd-halo" />
{:else if n.kind === 'pod'}
  <rect x={n.x} y={n.y} width={n.w} height={n.h} rx="2" class="wd-box" />
  <text x={n.x + (n.h > 60 ? 12 : 10)} y={n.y + (n.h > 60 ? 24 : 21)} class="wd-t-target">{n.title}</text>
  <text x={n.x + (n.h > 60 ? 12 : 10)} y={n.y + (n.h > 60 ? 41 : 40)} class="wd-t-sub">{n.sub}</text>
{:else if n.kind === 'chip'}
  <rect x={n.x} y={n.y} width={n.w} height={n.h} rx="2" />
  <text x={n.x + 12} y={n.y + 22} class={n.off ? 'wd-t-no' : 'wd-t-yes'}>{n.title}</text>
{:else}
  <rect x={n.x} y={n.y} width={n.w} height={n.h} rx="2" class={fill} />
  <rect x={n.x - 4} y={n.y - 4} width={n.w + 8} height={n.h + 8} rx="3" class="wd-halo" />
{/if}

{#if n.kind === 'state'}
  <text x={cx} y={n.y + n.h / 2 + 4} class="wd-t-code" text-anchor="middle">{n.glyph}&nbsp;&nbsp;{n.title}</text>
{:else if !['table', 'pod', 'chip'].includes(n.kind)}
  {#if n.sub}
    <text x={cx} y={n.y + n.h / 2 - 2} class={'wd-t-title' + ink} text-anchor="middle">{n.title}</text>
    <text x={cx} y={n.y + n.h / 2 + 14} class={'wd-t-sub' + ink} text-anchor="middle">{n.sub}</text>
  {:else}
    <text x={cx} y={n.y + n.h / 2 + 5} class={'wd-t-title' + ink} text-anchor="middle">{n.title}</text>
  {/if}
{/if}
