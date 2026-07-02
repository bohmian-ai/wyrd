<script lang="ts">
  // Reusable control-flow diagram for the auth docs. Two layouts:
  //   - flow (default): boxes stacked top-to-bottom, one arrow per hop.
  //   - fan: a root box (steps[0]) dispatching to N sibling branches (steps[1..])
  //     off a left spine — used for the /auth/token grant match.
  // Box color/geometry reuse the shared `.wyrd-diagram` token rules in
  // arcade.css (node-* / arrow-label classes) so light/dark both work. The arrow
  // marker is defined per instance with a title-derived id so multiple diagrams
  // on one page never collide on `#wyrd-arrow`.
  type Step = {
    /** Bold primary line — a route, type, or action. */
    label: string;
    /** Optional mono sub-line with detail. */
    sub?: string;
    /** Optional short label on the arrow leaving this step (flow only). */
    edge?: string;
    /** Render this box in the accent fill (a decision or terminal step). */
    accent?: boolean;
  };

  type Box = { x: number; y: number; w: number; h: number; cx: number } & Step;
  type Arrow = { d: string; edge?: string; ex: number; ey: number };

  let {
    title,
    steps,
    variant = 'flow'
  }: { title: string; steps: Step[]; variant?: 'flow' | 'fan' } = $props();

  const VW = 520;
  const PAD_X = 26;
  const BOX_W = VW - PAD_X * 2;
  const BOX_H = 56;
  const GAP = 34;
  const TOP = 8;

  // Fan geometry.
  const SPINE_X = 46;
  const LEAF_X = 72;
  const LEAF_W = VW - LEAF_X - PAD_X;
  const LEAF_GAP = 14;
  const ROOT_GAP = 28;

  const model = $derived.by(() => {
    const marker = `wf-arrow-${title.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
    const boxes: Box[] = [];
    const arrows: Arrow[] = [];
    let spine: string | null = null;
    let height = 0;

    if (variant === 'fan') {
      const root = steps[0];
      const leaves = steps.slice(1);
      boxes.push({ ...root, x: PAD_X, y: TOP, w: BOX_W, h: BOX_H, cx: VW / 2, accent: root.accent !== false });
      const firstY = TOP + BOX_H + ROOT_GAP;
      leaves.forEach((leaf, i) => {
        const y = firstY + i * (BOX_H + LEAF_GAP);
        const mid = y + BOX_H / 2;
        boxes.push({ ...leaf, x: LEAF_X, y, w: LEAF_W, h: BOX_H, cx: LEAF_X + LEAF_W / 2 });
        arrows.push({ d: `M ${SPINE_X} ${mid} L ${LEAF_X} ${mid}`, edge: leaf.edge, ex: LEAF_X + 4, ey: mid - 7 });
      });
      const lastMid = firstY + (leaves.length - 1) * (BOX_H + LEAF_GAP) + BOX_H / 2;
      spine = `M ${SPINE_X} ${TOP + BOX_H} L ${SPINE_X} ${lastMid}`;
      height = firstY + leaves.length * BOX_H + (leaves.length - 1) * LEAF_GAP + TOP;
    } else {
      steps.forEach((step, i) => {
        const y = TOP + i * (BOX_H + GAP);
        boxes.push({ ...step, x: PAD_X, y, w: BOX_W, h: BOX_H, cx: VW / 2 });
        if (i < steps.length - 1) {
          arrows.push({
            d: `M ${VW / 2} ${y + BOX_H} L ${VW / 2} ${y + BOX_H + GAP}`,
            edge: step.edge,
            ex: VW / 2 + 12,
            ey: y + BOX_H + GAP / 2 + 4
          });
        }
      });
      height = TOP * 2 + steps.length * BOX_H + (steps.length - 1) * GAP;
    }

    const ariaLabel =
      variant === 'fan'
        ? `${title}: ${steps[0].label} dispatches to ${steps.slice(1).map((s) => s.label).join(', ')}`
        : `${title}: ${steps.map((s) => s.label).join(', then ')}`;

    return { marker, boxes, arrows, spine, height, ariaLabel };
  });
</script>

<svg
  class="wyrd-diagram"
  viewBox={`0 0 ${VW} ${model.height}`}
  role="img"
  aria-label={model.ariaLabel}
  xmlns="http://www.w3.org/2000/svg"
>
  <defs>
    <marker id={model.marker} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
      <path d="M0,0 L10,5 L0,10 Z" fill="var(--border)" />
    </marker>
  </defs>

  {#if model.spine}
    <path class="wf-spine" d={model.spine} />
  {/if}

  {#each model.arrows as arrow (arrow.d)}
    <path class="wf-arrow" d={arrow.d} marker-end={`url(#${model.marker})`} />
    {#if arrow.edge}
      <text class="arrow-label" x={arrow.ex} y={arrow.ey}>{arrow.edge}</text>
    {/if}
  {/each}

  {#each model.boxes as box (box.y + '-' + box.x)}
    <rect class="node-shadow" x={box.x + 5} y={box.y + 5} width={box.w} height={box.h} />
    <rect class={box.accent ? 'node-fill-accent' : 'node-fill'} x={box.x} y={box.y} width={box.w} height={box.h} />
    <rect class="node-stroke" x={box.x} y={box.y} width={box.w} height={box.h} />
    <text
      class={box.accent ? 'node-text node-text-on-accent' : 'node-text'}
      x={box.cx}
      y={box.sub ? box.y + 24 : box.y + 33}
    >{box.label}</text>
    {#if box.sub}
      <text class={box.accent ? 'node-sub node-sub-on-accent' : 'node-sub'} x={box.cx} y={box.y + 42}
        >{box.sub}</text>
    {/if}
  {/each}
</svg>

<style>
  .wf-arrow,
  .wf-spine {
    fill: none;
    stroke: var(--border);
    stroke-width: 3;
  }
</style>
