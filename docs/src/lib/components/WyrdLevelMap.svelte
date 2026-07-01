<script lang="ts">
  import { base } from '$app/paths';

  // Landing intent-router: six coin-op "levels" routing into the current IA.
  // Replaces the old concept-walkthrough levelmap; pixel numerals are sized down
  // from the original (which read oversized in the column).
  type Plane = 'client' | 'server' | 'control' | 'rune';
  type Level = { num: string; name: string; desc: string; path: string; plane: Plane };

  const levels: Level[] = [
    {
      num: '01',
      name: 'Install & quickstart',
      desc: 'Install Wyrd and get a card registered against a running server in minutes.',
      path: '/start-here/quickstart/',
      plane: 'client'
    },
    {
      num: '02',
      name: 'How it connects',
      desc: 'Every component is a Card; CardRefs link them and Wyrd derives the lineage graph.',
      path: '/start-here/how-it-connects/',
      plane: 'control'
    },
    {
      num: '03',
      name: 'Cards',
      desc: 'The typed, versioned envelope for models, prompts, agents, services, and evals.',
      path: '/cards/',
      plane: 'rune'
    },
    {
      num: '04',
      name: 'Server & auth',
      desc: 'Run the registry, bind identity, and register cards with policy in front.',
      path: '/server/',
      plane: 'server'
    },
    {
      num: '05',
      name: 'Skald',
      desc: 'Build agents and workflows that run against the cards you declared.',
      path: '/skald/',
      plane: 'client'
    },
    {
      num: '06',
      name: 'Evaluation',
      desc: 'Score behavior against declared expectations and track drift over time.',
      path: '/evaluation/',
      plane: 'server'
    }
  ];
</script>

<section class="wyrd-levelmap" aria-label="Where to start">
  <div class="lm-head">
    <span class="lm-coin">&#9654;</span>
    <span class="lm-title">SELECT YOUR PATH</span>
    <span class="lm-sub">install &rarr; build &rarr; evaluate</span>
  </div>
  <ol class="lm-track">
    {#each levels as l (l.num)}
      <li class="lm-level" data-plane={l.plane}>
        <a href={`${base}${l.path}`}>
          <span class="lm-num">{l.num}</span>
          <span class="lm-name">{l.name}</span>
          <span class="lm-desc">{l.desc}</span>
        </a>
      </li>
    {/each}
  </ol>
</section>

<style>
  .wyrd-levelmap {
    margin: 2rem 0;
  }
  .lm-head {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 10px;
    margin-bottom: 1rem;
  }
  .lm-coin {
    color: var(--lime-text);
    font-size: 0.8rem;
  }
  .lm-title {
    font-family: var(--font-arcade);
    font-size: 0.8rem;
    letter-spacing: 0.5px;
    color: var(--text);
  }
  .lm-sub {
    font-family: var(--font-mono);
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .lm-track {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 12px;
  }
  .lm-level {
    margin: 0;
  }
  .lm-level a {
    display: flex;
    flex-direction: column;
    gap: 6px;
    height: 100%;
    text-decoration: none;
    color: var(--text);
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    border-top: 5px solid var(--rune-strong);
    padding: 12px 13px 13px;
    transition:
      transform 0.08s,
      box-shadow 0.08s;
  }
  .lm-level a:hover {
    transform: translate(-2px, -2px);
    box-shadow: 6px 6px 0 0 var(--shadow);
  }
  .lm-level[data-plane='client'] a {
    border-top-color: var(--client-bar);
  }
  .lm-level[data-plane='server'] a {
    border-top-color: var(--server-bar);
  }
  .lm-level[data-plane='control'] a {
    border-top-color: var(--control-bar);
  }
  .lm-level[data-plane='rune'] a {
    border-top-color: var(--rune-strong);
  }
  .lm-num {
    font-family: var(--font-pixel);
    /* sized down from the original 1.6rem so the numerals sit in the card */
    font-size: 1.15rem;
    line-height: 0.85;
    color: var(--rune-strong);
  }
  .lm-name {
    font-family: var(--font-display);
    font-size: 0.95rem;
    line-height: 1.15;
  }
  .lm-desc {
    font-family: var(--font-sans);
    font-size: 0.78rem;
    line-height: 1.45;
    color: var(--muted);
  }
</style>
