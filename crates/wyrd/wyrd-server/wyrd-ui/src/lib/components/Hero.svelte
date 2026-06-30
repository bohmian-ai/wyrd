<script lang="ts">
  type Stat = { value: string; label: string };
  type Cta = { label: string; variant?: 'lime' | 'rune'; href?: string };

  let {
    title,
    tagline,
    ornament,
    watermark = 'WYRD',
    stats,
    ctas
  }: {
    title: string;
    tagline?: string;
    ornament?: string;
    watermark?: string;
    stats?: Stat[];
    ctas?: Cta[];
  } = $props();
</script>

<section class="wy-hero">
  <div class="bar"></div>
  <div class="pad">
    {#if ornament}<span class="orn">{ornament}</span>{/if}
    <span class="wm">{watermark}<i></i></span>
    <div><span class="hero">{title}</span></div>
    {#if tagline}<p class="tag">{tagline}</p>{/if}
    {#if stats?.length}
      <div class="stats">
        {#each stats as s (s.label)}
          <div class="stat">
            <div class="n">{s.value}</div>
            <div class="k">{s.label}</div>
          </div>
        {/each}
      </div>
    {/if}
    {#if ctas?.length}
      <div class="ctas">
        {#each ctas as c (c.label)}
          {#if c.href}
            <a class="cta" data-v={c.variant ?? 'lime'} href={c.href}>{c.label}</a>
          {:else}
            <button class="cta" data-v={c.variant ?? 'lime'} type="button">{c.label}</button>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .wy-hero {
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    overflow: hidden;
    color: var(--text);
  }
  .bar {
    height: 8px;
    background: linear-gradient(
      90deg,
      var(--client-bar) 0 33.3%,
      var(--server-bar) 33.3% 66.6%,
      var(--control-bar) 66.6% 100%
    );
  }
  .pad {
    padding: 24px 24px 28px;
    position: relative;
  }
  .orn {
    float: right;
    font-family: var(--fm);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--rune-strong);
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 5px 9px;
    box-shadow: 3px 3px 0 0 var(--shadow);
    background: var(--surface);
  }
  .wm {
    font-family: var(--fh);
    font-size: 17px;
    letter-spacing: -0.3px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .wm i {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--lime);
  }
  .hero {
    display: inline-block;
    font-family: var(--fh);
    font-size: clamp(22px, 3.2vw, 33px);
    line-height: 1.03;
    letter-spacing: -0.5px;
    text-transform: uppercase;
    margin: 18px 0 0;
    padding: 11px 15px;
    border-radius: var(--r);
    background: var(--hero-bg);
    color: var(--hero-ink);
    border: 3px solid var(--border);
    box-shadow: 10px 10px 0 0 var(--shadow);
  }
  .tag {
    margin-top: 19px;
    font-size: 14px;
    line-height: 1.55;
    max-width: 46ch;
  }
  .stats {
    display: flex;
    gap: 13px;
    margin-top: 21px;
    flex-wrap: wrap;
  }
  .stat {
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 6px 6px 0 0 var(--shadow);
    padding: 13px 16px;
    flex: 1;
    min-width: 110px;
  }
  .stat .n {
    font-family: var(--fm);
    font-weight: 700;
    font-size: 26px;
    line-height: 1;
    color: var(--rune-strong);
  }
  .stat .k {
    font-family: var(--fm);
    font-size: 9px;
    font-weight: 500;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    color: var(--muted);
    margin-top: 7px;
  }
  .ctas {
    margin-top: 24px;
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .cta {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-family: var(--fm);
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.3px;
    padding: 13px 22px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--lime);
    color: var(--lime-ink);
    text-decoration: none;
    cursor: pointer;
    box-shadow: 6px 6px 0 0 var(--shadow);
    transition:
      transform 0.1s,
      box-shadow 0.1s;
  }
  .cta:hover {
    transform: translate(-2px, -2px);
    box-shadow: 9px 9px 0 0 var(--shadow);
  }
  .cta:active {
    transform: translate(3px, 3px);
    box-shadow: 2px 2px 0 0 var(--shadow);
  }
  .cta[data-v='rune'] {
    background: var(--rune-btn);
    color: var(--rune-btn-ink);
  }
</style>
