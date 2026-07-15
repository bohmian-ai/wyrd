<script lang="ts">
  // Bifrost system diagram. SVG (not nested HTML grids) so it scales
  // predictably inside the 88ch prose column and never overflows the page.
  // Every color pulls from design tokens so light/dark both work.

  const VW = 960;

  // Outer frame
  const PAD = 12;
  const LEFT_W = 118;
  const RIGHT_W = 118;
  const COL_GAP = 14;
  const POD_X = PAD + LEFT_W + COL_GAP;
  const POD_W = VW - PAD * 2 - LEFT_W - RIGHT_W - COL_GAP * 2;
  const RIGHT_X = VW - PAD - RIGHT_W;

  // Left / right side panels
  const SIDE_TOP = PAD;
  const SIDE_H = 292;
  const CAP_H = 22;
  const BOX_H = 46;
  const BOX_GAP = 10;

  // Pod interior
  const POD_TOP = PAD;
  const POD_INSET = 12;
  const POD_LABEL_Y = POD_TOP + 4;

  // Gate row
  const GATE_TOP = POD_TOP + 22;
  const GATE_H = 66;
  const GATE_X = POD_X + POD_INSET;
  const GATE_W = POD_W - POD_INSET * 2;

  // Roles row (Scribe / Oracle / Forge). Sized to fit Scribe's 4 stacked parts;
  // Oracle and Forge (3 parts each) leave a bit of breathing room at the bottom.
  const ROLES_TOP = GATE_TOP + GATE_H + 12;
  const ROLES_H = 232;
  const ROLES_GAP = 10;
  const ROLE_W = (GATE_W - ROLES_GAP * 2) / 3;
  const SCRIBE_X = GATE_X;
  const ORACLE_X = SCRIBE_X + ROLE_W + ROLES_GAP;
  const FORGE_X = ORACLE_X + ROLE_W + ROLES_GAP;

  // Role internal layout
  const ROLE_HEAD_H = 34;
  const ROLE_PART_H = 40;
  const ROLE_PART_GAP = 6;
  const ROLE_INNER_PAD = 10;

  // Pod height
  const POD_H = (ROLES_TOP - POD_TOP) + ROLES_H + POD_INSET;

  // State row (bottom)
  const STATE_TOP = Math.max(SIDE_TOP + SIDE_H, POD_TOP + POD_H) + 22;
  const STATE_H = 74;
  const STATE_GAP = 14;
  const STATE_W = (VW - PAD * 2 - STATE_GAP * 2) / 3;

  const totalH = STATE_TOP + STATE_H + PAD;

  // Labels are arrays of lines. Multi-line entries render as two <tspan>s so
  // longer names (e.g. "Wyrd identity") stack instead of hugging the box edge.
  const sources: string[][] = [['OTLP'], ['HTTP · gRPC'], ['SDK'], ['MCP']];
  const authItems: string[][] = [['Wyrd', 'identity'], ['policy'], ['audit']];

  const scribeParts = [
    { t: 'WAL', s: 'Arrow IPC · fsync' },
    { t: 'Memtable', s: 'rows in RAM' },
    { t: 'Seal', s: 'Parquet · PUT' },
    { t: 'INSERT vala.file_list', s: '+ audit (atomic)' }
  ];

  const oracleParts = [
    { t: 'Iceberg snapshots', s: 'compacted history' },
    { t: 'Un-compacted Parquet', s: 'from vala.file_list' },
    { t: 'Live Scribe tail', s: 'sub-seal freshness' }
  ];

  const forgeParts = [
    { t: 'FILE_COMPACT', s: 'bin-pack Parquet' },
    { t: 'Iceberg REPLACE', s: 'single writer' },
    { t: 'SNAPSHOT_EXPIRE', s: '· ORPHAN_GC' }
  ];

  const state = [
    {
      icon: 'PG',
      name: 'Postgres',
      lines: ['vala.file_list · audit_outbox', 'vala.maintenance_leases (Forge)'],
      accent: true
    },
    { icon: '◧', name: 'Object store', lines: ['Parquet · S3 · GCS · Azure'], accent: false },
    { icon: '✦', name: 'Iceberg catalog', lines: ['snapshots · Forge writes only'], accent: false }
  ];

  // All three roles now stack vertically so each part gets the full role-column
  // width and monospace text can't spill past the part borders.
  const rolePartY = (i: number) => ROLES_TOP + ROLE_HEAD_H + 6 + i * (ROLE_PART_H + ROLE_PART_GAP);
</script>

<figure class="bpa">
  <svg
    viewBox={`0 0 ${VW} ${totalH}`}
    role="img"
    aria-label="Bifrost system diagram. One Bifrost pod runs Gate, Scribe, Oracle, and Forge together. Sources on the left (OTLP, HTTP/gRPC, SDK, MCP) enter Gate, which routes writes to Scribe and reads to Oracle. Forge compacts under a Postgres lease. Auth (Wyrd identity, policy, audit) sits on the right. Durable state lives in Postgres, object store, and the Iceberg catalog."
    xmlns="http://www.w3.org/2000/svg"
  >
    <!-- Left panel: sources -->
    <g>
      <rect class="bpa-panel" x={PAD} y={SIDE_TOP} width={LEFT_W} height={SIDE_H} rx="5" />
      <text class="bpa-cap" x={PAD + LEFT_W / 2} y={SIDE_TOP + CAP_H - 4}>SOURCES</text>
      {#each sources as lines, i (lines[0])}
        {@const y = SIDE_TOP + CAP_H + 6 + i * (BOX_H + BOX_GAP)}
        {@const cx = PAD + LEFT_W / 2}
        {@const cy = y + BOX_H / 2 + 4 - (lines.length - 1) * 7}
        <rect
          class="bpa-box bpa-box-lime"
          x={PAD + 10}
          y={y}
          width={LEFT_W - 20}
          height={BOX_H}
          rx="5"
        />
        <text class="bpa-box-t bpa-lime-t" x={cx} y={cy}>
          {#each lines as line, li (line)}
            <tspan x={cx} dy={li === 0 ? 0 : 13}>{line}</tspan>
          {/each}
        </text>
      {/each}
    </g>

    <!-- Center: Bifrost pod -->
    <g>
      <rect class="bpa-pod" x={POD_X} y={POD_TOP} width={POD_W} height={POD_H} rx="5" />
      <rect
        class="bpa-pod-lab-bg"
        x={POD_X + 12}
        y={POD_TOP - 8}
        width="90"
        height="16"
        rx="3"
      />
      <text class="bpa-pod-lab" x={POD_X + 18} y={POD_TOP + 3}>BIFROST POD</text>

      <!-- Gate -->
      <rect
        class="bpa-role-shadow"
        x={GATE_X + 3}
        y={GATE_TOP + 3}
        width={GATE_W}
        height={GATE_H}
        rx="5"
      />
      <rect
        class="bpa-role bpa-role-server"
        x={GATE_X}
        y={GATE_TOP}
        width={GATE_W}
        height={GATE_H}
        rx="5"
      />
      <text class="bpa-role-name bpa-name-server" x={GATE_X + 14} y={GATE_TOP + 24}>Gate</text>
      <text class="bpa-role-detail" x={GATE_X + 14} y={GATE_TOP + 44}
        >authenticate · resolve tenant + table · route</text
      >
      <text class="bpa-role-detail" x={GATE_X + 14} y={GATE_TOP + 58}
        ><tspan class="bpa-em">write › Scribe</tspan><tspan dx="8"
          ><tspan class="bpa-em">read › Oracle</tspan></tspan
        ></text
      >

      <!-- Scribe -->
      <rect
        class="bpa-role-shadow"
        x={SCRIBE_X + 3}
        y={ROLES_TOP + 3}
        width={ROLE_W}
        height={ROLES_H}
        rx="5"
      />
      <rect
        class="bpa-role bpa-role-control"
        x={SCRIBE_X}
        y={ROLES_TOP}
        width={ROLE_W}
        height={ROLES_H}
        rx="5"
      />
      <text class="bpa-role-name bpa-name-control" x={SCRIBE_X + 12} y={ROLES_TOP + 22}>Scribe</text>
      <text class="bpa-role-sub" x={SCRIBE_X + ROLE_W - 12} y={ROLES_TOP + 22} text-anchor="end"
        >WRITE · DURABLE ACK</text
      >

      {#each scribeParts as p, i (p.t)}
        <rect
          class="bpa-part bpa-part-control"
          x={SCRIBE_X + ROLE_INNER_PAD}
          y={rolePartY(i)}
          width={ROLE_W - ROLE_INNER_PAD * 2}
          height={ROLE_PART_H}
          rx="5"
        />
        <text class="bpa-part-t bpa-part-t-control" x={SCRIBE_X + ROLE_W / 2} y={rolePartY(i) + 16}
          >{p.t}</text
        >
        <text class="bpa-part-s" x={SCRIBE_X + ROLE_W / 2} y={rolePartY(i) + 30}>{p.s}</text>
      {/each}

      <!-- Oracle -->
      <rect
        class="bpa-role-shadow"
        x={ORACLE_X + 3}
        y={ROLES_TOP + 3}
        width={ROLE_W}
        height={ROLES_H}
        rx="5"
      />
      <rect
        class="bpa-role bpa-role-server"
        x={ORACLE_X}
        y={ROLES_TOP}
        width={ROLE_W}
        height={ROLES_H}
        rx="5"
      />
      <text class="bpa-role-name bpa-name-server" x={ORACLE_X + 12} y={ROLES_TOP + 22}>Oracle</text>
      <text class="bpa-role-sub" x={ORACLE_X + ROLE_W - 12} y={ROLES_TOP + 22} text-anchor="end"
        >READ · FUSED SCAN</text
      >

      {#each oracleParts as p, i (p.t)}
        <rect
          class="bpa-part bpa-part-server"
          x={ORACLE_X + ROLE_INNER_PAD}
          y={rolePartY(i)}
          width={ROLE_W - ROLE_INNER_PAD * 2}
          height={ROLE_PART_H}
          rx="5"
        />
        <text class="bpa-part-t bpa-part-t-server" x={ORACLE_X + ROLE_W / 2} y={rolePartY(i) + 16}
          >{p.t}</text
        >
        <text class="bpa-part-s" x={ORACLE_X + ROLE_W / 2} y={rolePartY(i) + 30}>{p.s}</text>
      {/each}

      <!-- Forge -->
      <rect
        class="bpa-role-shadow"
        x={FORGE_X + 3}
        y={ROLES_TOP + 3}
        width={ROLE_W}
        height={ROLES_H}
        rx="5"
      />
      <rect
        class="bpa-role bpa-role-rune"
        x={FORGE_X}
        y={ROLES_TOP}
        width={ROLE_W}
        height={ROLES_H}
        rx="5"
      />
      <text class="bpa-role-name bpa-name-rune" x={FORGE_X + 12} y={ROLES_TOP + 22}>Forge</text>
      <text class="bpa-role-sub" x={FORGE_X + ROLE_W - 12} y={ROLES_TOP + 22} text-anchor="end"
        >COMPACT · PG-LEASED</text
      >

      {#each forgeParts as p, i (p.t)}
        <rect
          class="bpa-part bpa-part-rune"
          x={FORGE_X + ROLE_INNER_PAD}
          y={rolePartY(i)}
          width={ROLE_W - ROLE_INNER_PAD * 2}
          height={ROLE_PART_H}
          rx="5"
        />
        <text class="bpa-part-t bpa-part-t-rune" x={FORGE_X + ROLE_W / 2} y={rolePartY(i) + 16}
          >{p.t}</text
        >
        <text class="bpa-part-s" x={FORGE_X + ROLE_W / 2} y={rolePartY(i) + 30}>{p.s}</text>
      {/each}
    </g>

    <!-- Right panel: auth -->
    <g>
      <rect class="bpa-panel" x={RIGHT_X} y={SIDE_TOP} width={RIGHT_W} height={SIDE_H} rx="5" />
      <text class="bpa-cap" x={RIGHT_X + RIGHT_W / 2} y={SIDE_TOP + CAP_H - 4}>AUTH</text>
      {#each authItems as lines, i (lines[0])}
        {@const y = SIDE_TOP + CAP_H + 6 + i * (BOX_H + BOX_GAP)}
        {@const cx = RIGHT_X + RIGHT_W / 2}
        {@const cy = y + BOX_H / 2 + 4 - (lines.length - 1) * 7}
        <rect
          class="bpa-box bpa-box-rune"
          x={RIGHT_X + 10}
          y={y}
          width={RIGHT_W - 20}
          height={BOX_H}
          rx="5"
        />
        <text class="bpa-box-t bpa-rune-t" x={cx} y={cy}>
          {#each lines as line, li (line)}
            <tspan x={cx} dy={li === 0 ? 0 : 13}>{line}</tspan>
          {/each}
        </text>
      {/each}
    </g>

    <!-- Bottom: durable state row -->
    <g>
      {#each state as s, i (s.name)}
        {@const x = PAD + i * (STATE_W + STATE_GAP)}
        <rect
          class={s.accent ? 'bpa-db bpa-db-rune' : 'bpa-db'}
          x={x}
          y={STATE_TOP}
          width={STATE_W}
          height={STATE_H}
          rx="5"
        />
        <text
          class={s.accent ? 'bpa-db-icon bpa-db-icon-rune' : 'bpa-db-icon'}
          x={x + STATE_W / 2}
          y={STATE_TOP + 22}>{s.icon}</text
        >
        <text class="bpa-db-name" x={x + STATE_W / 2} y={STATE_TOP + 42}>{s.name}</text>
        {#each s.lines as line, li (line)}
          <text class="bpa-db-role" x={x + STATE_W / 2} y={STATE_TOP + 56 + li * 12}>{line}</text>
        {/each}
      {/each}
    </g>
  </svg>
</figure>

<style>
  .bpa {
    margin: 1.5rem 0;
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    padding: 0;
  }
  .bpa svg {
    display: block;
    width: 100%;
    height: auto;
  }

  /* Side panels (sources, auth) */
  .bpa-panel {
    fill: var(--surface);
    stroke: var(--border);
    stroke-width: 2;
  }
  .bpa-cap {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 1.4px;
    text-anchor: middle;
  }
  .bpa-box {
    fill: var(--surface-2);
    stroke: var(--border);
    stroke-width: 2;
  }
  .bpa-box-lime {
    stroke: var(--lime);
  }
  .bpa-box-rune {
    stroke: var(--rune-strong);
  }
  .bpa-box-t {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    text-anchor: middle;
  }
  .bpa-lime-t {
    fill: var(--lime-text);
  }
  .bpa-rune-t {
    fill: var(--rune-strong);
  }

  /* Pod */
  .bpa-pod {
    fill: var(--surface);
    stroke: var(--border);
    stroke-width: 2;
  }
  .bpa-pod-lab-bg {
    fill: var(--surface);
  }
  .bpa-pod-lab {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 1.4px;
  }

  /* Roles */
  .bpa-role {
    fill: var(--surface-2);
    stroke-width: 2;
  }
  .bpa-role-shadow {
    fill: var(--shadow);
  }
  .bpa-role-server {
    stroke: var(--server);
  }
  .bpa-role-control {
    stroke: var(--control);
  }
  .bpa-role-rune {
    stroke: var(--rune-strong);
  }
  .bpa-role-name {
    font-family: var(--font-display);
    font-size: 16px;
    font-weight: 700;
  }
  .bpa-name-server {
    fill: var(--server);
  }
  .bpa-name-control {
    fill: var(--control);
  }
  .bpa-name-rune {
    fill: var(--rune-strong);
  }
  .bpa-role-sub {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.5px;
  }
  .bpa-role-detail {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .bpa-em {
    fill: var(--text);
    font-weight: 700;
  }

  /* Parts */
  .bpa-part {
    fill: var(--surface);
    stroke-width: 1.5;
  }
  .bpa-part-server {
    stroke: var(--server);
  }
  .bpa-part-control {
    stroke: var(--control);
  }
  .bpa-part-rune {
    stroke: var(--rune-strong);
  }
  .bpa-part-t {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    text-anchor: middle;
  }
  .bpa-part-t-server {
    fill: var(--server);
  }
  .bpa-part-t-control {
    fill: var(--control);
  }
  .bpa-part-t-rune {
    fill: var(--rune-strong);
  }
  .bpa-part-s {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 9.5px;
    text-anchor: middle;
  }

  /* State row */
  .bpa-db {
    fill: var(--surface);
    stroke: var(--border);
    stroke-width: 2;
  }
  .bpa-db-rune {
    stroke: var(--rune-strong);
  }
  .bpa-db-icon {
    fill: var(--rune-strong);
    font-family: var(--font-display);
    font-size: 18px;
    font-weight: 700;
    text-anchor: middle;
    letter-spacing: 1px;
  }
  .bpa-db-icon-rune {
    fill: var(--rune-strong);
  }
  .bpa-db-name {
    fill: var(--text);
    font-family: var(--font-display);
    font-size: 14px;
    font-weight: 700;
    text-anchor: middle;
  }
  .bpa-db-role {
    fill: var(--muted);
    font-family: var(--font-mono);
    font-size: 10.5px;
    text-anchor: middle;
    letter-spacing: 0.3px;
  }
</style>
