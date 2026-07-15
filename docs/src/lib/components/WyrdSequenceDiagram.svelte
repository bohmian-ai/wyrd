<script lang="ts">
  // UML-style sequence diagram for the auth docs. Participants get lifelines;
  // messages are horizontal arrows between them, top to bottom in time order.
  //   - call: solid arrow (default).
  //   - return: dashed arrow (`dashed: true`).
  //   - self message: from === to renders a loop on the participant's own lifeline.
  // Participant boxes reuse the accent `.wyrd-diagram` node tokens (node-fill-accent
  // / node-text-on-accent) so they match the brand and flip for light/dark. The
  // arrowhead marker id is derived from the title so multiple diagrams on one page
  // never collide.
  type Message = {
    /** Source participant name (must be in `participants`). */
    from: string;
    /** Target participant name. Equal to `from` for a self message. */
    to: string;
    /** Message label rendered above the arrow. */
    label: string;
    /** Render as a dashed return arrow. */
    dashed?: boolean;
  };

  let {
    title,
    participants,
    messages,
    dense = false
  }: { title: string; participants: string[]; messages: Message[]; dense?: boolean } = $props();

  // `dense` shrinks participant boxes, column gap, and row height so a data-plane
  // flow with 5-6 participants and 10+ messages doesn't blow up vertically.
  // Auth pages leave `dense` unset and get the original geometry.
  const PART_W = $derived(dense ? 96 : 122);
  const PART_H = $derived(dense ? 32 : 40);
  const COL_GAP = $derived(dense ? 140 : 190);
  const LEFT = $derived(PART_W / 2 + 14);
  const TOP_GAP = $derived(dense ? 22 : 30);
  const ROW_H = $derived(dense ? 34 : 48);
  const SELF_W = $derived(dense ? 36 : 46);
  const SELF_H = $derived(dense ? 20 : 26);
  const SELF_ADVANCE = $derived(SELF_H + (dense ? 18 : 24));
  const CHAR_W = $derived(dense ? 5.7 : 6.4);

  const model = $derived.by(() => {
    const marker = `sq-arrow-${title.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
    const colX = (name: string) => LEFT + participants.indexOf(name) * COL_GAP;
    const baseVw = LEFT * 2 + (participants.length - 1) * COL_GAP;

    const arrows: {
      d: string;
      dashed: boolean;
      label: string;
      lx: number;
      ly: number;
      anchor: 'middle' | 'start';
    }[] = [];

    let y = PART_H + TOP_GAP;
    let maxRight = baseVw;
    let minLeft = 0;
    for (const m of messages) {
      const est = m.label.length * CHAR_W;
      const x1 = colX(m.from);
      if (m.from === m.to) {
        maxRight = Math.max(maxRight, x1 + SELF_W + 8 + est);
        arrows.push({
          d: `M ${x1} ${y} L ${x1 + SELF_W} ${y} L ${x1 + SELF_W} ${y + SELF_H} L ${x1} ${y + SELF_H}`,
          dashed: m.dashed === true,
          label: m.label,
          lx: x1 + SELF_W + 8,
          ly: y + SELF_H / 2 + 3,
          anchor: 'start'
        });
        y += SELF_ADVANCE;
      } else {
        const x2 = colX(m.to);
        const mid = (x1 + x2) / 2;
        maxRight = Math.max(maxRight, mid + est / 2);
        minLeft = Math.min(minLeft, mid - est / 2);
        arrows.push({
          d: `M ${x1} ${y} L ${x2} ${y}`,
          dashed: m.dashed === true,
          label: m.label,
          lx: mid,
          ly: y - 8,
          anchor: 'middle'
        });
        y += ROW_H;
      }
    }
    const minX = Math.min(0, minLeft - 8);
    const vw = Math.max(baseVw, maxRight + 14) - minX;

    const lifelineBottom = y - ROW_H + 24;
    const boxes = participants.map((name) => ({ name, x: colX(name) - PART_W / 2 }));
    const height = lifelineBottom + PART_H + 8;

    const ariaLabel = `${title}: sequence between ${participants.join(', ')}. ${messages
      .map((m) => `${m.from} ${m.dashed ? 'returns to' : 'to'} ${m.to}: ${m.label}`)
      .join('; ')}`;

    return { marker, minX, vw, height, arrows, boxes, lifelineBottom, ariaLabel };
  });
</script>

<svg
  class={dense ? 'wyrd-diagram sq-dense' : 'wyrd-diagram'}
  viewBox={`${model.minX} 0 ${model.vw} ${model.height}`}
  role="img"
  aria-label={model.ariaLabel}
  xmlns="http://www.w3.org/2000/svg"
>
  <defs>
    <marker id={model.marker} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
      <path d="M0,0 L10,5 L0,10 Z" fill="var(--border)" />
    </marker>
  </defs>

  {#each model.boxes as box (box.name)}
    <line class="sq-lifeline" x1={box.x + PART_W / 2} y1={PART_H} x2={box.x + PART_W / 2} y2={model.lifelineBottom} />
    <rect class="node-shadow" x={box.x + 4} y={4} width={PART_W} height={PART_H} />
    <rect class="node-fill-accent" x={box.x} y={0} width={PART_W} height={PART_H} />
    <rect class="node-stroke" x={box.x} y={0} width={PART_W} height={PART_H} />
    <text class="node-text node-text-on-accent" x={box.x + PART_W / 2} y={dense ? 21 : 25}>{box.name}</text>
    <rect class="node-shadow" x={box.x + 4} y={model.lifelineBottom + 4} width={PART_W} height={PART_H} />
    <rect class="node-fill-accent" x={box.x} y={model.lifelineBottom} width={PART_W} height={PART_H} />
    <rect class="node-stroke" x={box.x} y={model.lifelineBottom} width={PART_W} height={PART_H} />
    <text class="node-text node-text-on-accent" x={box.x + PART_W / 2} y={model.lifelineBottom + (dense ? 21 : 25)}>{box.name}</text>
  {/each}

  {#each model.arrows as arrow (arrow.d)}
    <path
      class={arrow.dashed ? 'sq-line dashed' : 'sq-line'}
      d={arrow.d}
      marker-end={`url(#${model.marker})`}
    />
    <text class="sq-msg" x={arrow.lx} y={arrow.ly} text-anchor={arrow.anchor}>{arrow.label}</text>
  {/each}
</svg>

<style>
  .sq-line {
    fill: none;
    stroke: var(--border);
    stroke-width: 2.5;
  }
  .sq-line.dashed {
    stroke-dasharray: 6 4;
  }
  .sq-lifeline {
    stroke: var(--border);
    stroke-width: 1.5;
    opacity: 0.45;
  }
  .sq-msg {
    fill: var(--text);
    font-family: var(--font-mono);
    font-size: 10.5px;
  }
  :global(.wyrd-diagram.sq-dense) .node-text {
    font-size: 12px;
  }
  .sq-dense .sq-msg {
    font-size: 9.5px;
  }
  .sq-dense .sq-line {
    stroke-width: 2;
  }
</style>
