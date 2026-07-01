<script lang="ts">
  import { base } from '$app/paths';

  // "What Wyrd does" — four one-paragraph pitches at the raised altitude (6px
  // shadow). Each tile carries a Line accent (client/server/control/rune) and
  // links into the section it pitches.
  type Plane = 'client' | 'server' | 'control' | 'rune';
  type Cap = { title: string; body: string; path: string; plane: Plane };

  const caps: Cap[] = [
    {
      title: 'Cards',
      body: 'Declare every model, prompt, agent, service, dataset, policy, and eval as one typed, versioned envelope. The kind selects the spec; Wyrd validates it and links its CardRefs into lineage.',
      path: '/cards/',
      plane: 'rune'
    },
    {
      title: 'Skald agents & workflows',
      body: 'Build agents and multi-step workflows that run against the cards you declared, in Python or Rust, with observation captured back onto the same model.',
      path: '/skald/',
      plane: 'client'
    },
    {
      title: 'Server & registry',
      body: 'Run the registry that versions cards, binds identity, enforces policy, and derives the lineage graph — headless, with the CLI, MCP, and clients all on the same API.',
      path: '/server/',
      plane: 'server'
    },
    {
      title: 'Evaluation',
      body: 'Score behavior against declared expectations, compare versions, and track drift over time — bound to the exact card versions that produced each result.',
      path: '/evaluation/',
      plane: 'control'
    }
  ];
</script>

<ul class="cap-grid">
  {#each caps as c (c.title)}
    <li class="cap" data-plane={c.plane}>
      <a href={`${base}${c.path}`}>
        <span class="cap-title">{c.title}</span>
        <span class="cap-body">{c.body}</span>
      </a>
    </li>
  {/each}
</ul>

<style>
  .cap-grid {
    list-style: none;
    margin: 1.5rem 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 14px;
  }
  .cap {
    margin: 0;
  }
  .cap a {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    text-decoration: none;
    color: var(--text);
    background: var(--surface);
    border: 2px solid var(--border);
    border-radius: var(--r);
    /* raised altitude — one step above the quiet surfaces around it */
    box-shadow: 6px 6px 0 0 var(--shadow);
    border-top: 5px solid var(--rune-strong);
    padding: 15px 16px 16px;
    transition:
      transform 0.08s,
      box-shadow 0.08s;
  }
  .cap a:hover {
    transform: translate(-2px, -2px);
    box-shadow: 9px 9px 0 0 var(--shadow);
  }
  .cap[data-plane='client'] a {
    border-top-color: var(--client-bar);
  }
  .cap[data-plane='server'] a {
    border-top-color: var(--server-bar);
  }
  .cap[data-plane='control'] a {
    border-top-color: var(--control-bar);
  }
  .cap[data-plane='rune'] a {
    border-top-color: var(--rune-strong);
  }
  .cap-title {
    font-family: var(--font-display);
    font-size: 1.05rem;
    line-height: 1.15;
  }
  .cap-body {
    font-family: var(--font-sans);
    font-size: 0.82rem;
    line-height: 1.5;
    color: var(--muted);
  }
</style>
