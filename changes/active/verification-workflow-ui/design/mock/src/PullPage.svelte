<script>
  import Lifecycle from './Lifecycle.svelte';
  import { REPOS, CONN } from './catalog.js';
  import { ui, execs, setsOf, execHref, pullHref, failedOf, verifierHref } from './store.svelte.js';
  import { when } from './data.js';

  // One pull request's review sets. The plain link follows the latest set; a `set` in the link pins that set.
  const r = $derived(REPOS.find((x) => x.card === ui.card));
  const list = $derived(setsOf(r.card, ui.pr));
  const latest = $derived(list[0]);
  const set = $derived(list.find((s) => s.id === ui.set) ?? latest);
  const pinned = $derived(!!ui.set);
  const members = $derived(set.members.map((id) => execs[id]));
</script>

<div class="xhead">
  <div class="xtitle">
    <div class="xname"><span class="kt">Pull request</span><h1 data-testid="pr-title">{set.title}</h1></div>
    <div class="xmeta">
      <a href="#/repositories/{r.card}" data-testid="pr-repo">{r.full}</a><span class="sep">·</span>#{ui.pr}<span class="sep">·</span>Repository Card {r.card}
    </div>
  </div>
  <div class="xactions">
    <a class="btn btn-sm" href="https://{CONN.host}/{r.full}/pull/{ui.pr}" target="_blank" rel="noopener noreferrer">{r.full}#{ui.pr} ↗</a>
  </div>
</div>

<section class="section" aria-labelledby="rs-h">
  <div class="section-head"><h2 class="h" id="rs-h">Review sets · {list.length}</h2></div>
  <nav class="sets" aria-label="Review sets of #{ui.pr}" data-testid="sets">
    {#each list as s (s.id)}
      <a class="setrow" href={pullHref(r.card, ui.pr, s.id === latest.id ? '' : s.id)} aria-current={s.id === set.id ? 'true' : undefined} data-set={s.id}>
        <span class="mono">{s.id}</span>
        <span class="small">{s.occurrence}</span>
        <span class="mono small">head {s.head}</span>
        <span class="mono small muted">{when(s.at)} UTC</span>
        <span class="mono small muted">{r.card}@{s.policy}</span>
        <span class="small muted push">{s.members.length} {s.members.length === 1 ? 'execution' : 'executions'}{s.id === latest.id ? ' · latest' : ''}</span>
      </a>
    {/each}
  </nav>
</section>

{#if pinned && set.id !== latest.id}
  <div class="banner info" role="status" data-testid="pinned">You are viewing earlier review set {set.id}. This link stays on it. <a href={pullHref(r.card, ui.pr)} data-testid="to-latest">Latest set {latest.id}</a></div>
{:else if pinned}
  <div class="banner info" role="status" data-testid="pinned">Pinned to {set.id}, the latest set when this link was made. <a href={pullHref(r.card, ui.pr)}>Follow the latest set</a></div>
{/if}

<section class="section" aria-labelledby="m-h">
  <div class="section-head"><h2 class="h" id="m-h">{set.id} · {set.occurrence} · head {set.head}</h2></div>
  {#if set.note}<p class="note">{set.note}</p>{/if}
  <div class="outs-wrap">
    <table class="outs" data-testid="members">
      <thead><tr><th scope="col">Execution</th><th scope="col">Verifier</th><th scope="col">Status</th><th scope="col">Verdict</th><th scope="col">Failed requirements</th></tr></thead>
      <tbody>
        {#each members as x (x.id)}
          <tr data-member={x.id}>
            <td class="cell"><div class="cellbox"><a class="mono" href={execHref(x.id)} data-exe={x.id}>{x.id}</a><span class="sub">{when(x.at)} UTC</span></div></td>
            <td class="cell"><div class="cellbox"><a class="mono" href={verifierHref(x.def)}>{x.def}</a></div></td>
            <td class="cell"><div class="cellbox"><Lifecycle {x} part="status" /></div></td>
            <td class="cell"><div class="cellbox"><Lifecycle {x} part="verdict" /></div></td>
            <td class="cell"><div class="cellbox"><span class="mono c-danger">{failedOf(x).join(', ')}</span>{#if !failedOf(x).length}<span class="muted mono">—</span>{/if}</div></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="note" data-testid="no-combined">Each Verifier records its own status and verdict. A review set has no combined verdict.</p>
</section>
