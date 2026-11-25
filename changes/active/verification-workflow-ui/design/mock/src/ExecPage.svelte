<script>
  import Graph from './Graph.svelte';
  import Inspector from './Inspector.svelte';
  import { LIFECYCLE, VERDICT, ACTION, when, day } from './data.js';
  import { ui, execs, sets, current, select, execLink, execHref, cancel, cancellable, blockedOf, admin, verifierHref, pullHref, verifyHref, cardOfRepo } from './store.svelte.js';

  // One execution page for every Verifier: the same header, graph and inspector.
  let { mode } = $props();
  const def = $derived(current().def);
  const x = $derived(current().exec);
  const life = $derived(LIFECYCLE[x.status]);
  const judged = $derived(Object.keys(def.reqs).filter((r) => x.reqs[r]).length);
  const outputCount = $derived(Object.values(x.steps).reduce((n, s) => n + s.inst.reduce((m, i) => m + i.outs.length, 0), 0));
  const blocked = $derived(blockedOf(x));
  const windows = $derived(def.windowed ? Object.values(execs).filter((e) => e.def === def.key).sort((a, b) => a.at - b.at) : []);
  const wi = $derived(windows.findIndex((e) => e.id === x.id));
  // A session judgment draws its own pinned members, so its graph is keyed by execution as well as definition.
  const graphKey = $derived(def.key + (x.members ? x.id : ''));
  const unfinished = $derived(['pending', 'running'].includes(x.status));
  const set = $derived(x.set && sets.find((s) => s.id === x.set));
  const latest = $derived(set && sets.filter((s) => !s.hidden && s.card === set.card && s.pr === set.pr).sort((a, b) => b.at - a.at)[0]);

  /** Names the page the back link returns to, read from the hash it carries. */
  function backLabel(h) {
    const [path] = h.slice(1).split('?');
    const p = path.split('/').filter(Boolean).map(decodeURIComponent);
    if (p[0] === 'x' && execs[p[1]]) { const e = execs[p[1]]; return `${e.def.split('@')[0]} · ${e.session ?? e.run ?? e.id}`; }
    if (p[0] === 'cards' && p[1] === 'verifiers' && p[2]) return p[2];
    if (p[0] === 'repositories' && p[2] === 'pulls') return `PR #${p[3]}`;
    if (p[0] === 'repositories' && p[1]) return p[1];
    if (p[0] === 'repositories') return 'Repositories';
    return 'Verify';
  }
  const back = $derived(ui.back || '#/verify');
  const trigger = $derived(x.repo ? 'GitHub App' : x.members ? `session ${x.session} closed` : x.window ? 'daily schedule 00:00 UTC' : x.run ? `${x.service} Application Run` : 'alice (manual)');

  let copied = $state(false);
  let fallback = $state('');
  async function copyLink() {
    const href = `${location.origin}${location.pathname}${execLink()}`;
    try { await navigator.clipboard.writeText(href); copied = true; setTimeout(() => (copied = false), 1600); }
    catch { fallback = href; }
  }
  // Drift windows keep the selection and where you came from.
  const toWindow = (e) => execHref(e.id, { sel: ui.sel, back: ui.back });
</script>

<div class="exec" data-testid="exec">
  <div class="xhead">
    <div class="xtitle">
      <div class="xname">
        <h1><a href={verifierHref(def.key)} data-testid="verifier-link" title="Open the exact Verifier definition">{def.name}<span class="muted">@{def.version}</span></a></h1>
        <span class="kt">Verifier</span>
      </div>
      <p class="desc">{def.description}</p>
      <div class="xmeta" data-testid="xmeta">
        execution <span class="mono" data-testid="exe-id">{x.id}</span>
        {#if x.repo}
          <span class="sep">·</span>repository <a href="#/repositories/{cardOfRepo(x.repo)}" data-testid="repo-link">{x.repo}</a>
          <span class="sep">·</span><a href={pullHref(x.card, x.pr)} data-testid="pr-link">PR #{x.pr}</a> {x.title}
          <span class="sep">·</span>head <span class="mono">{x.head}</span>
          {#if set}<span class="sep">·</span>review set <a href={pullHref(x.card, x.pr, set.id)} data-testid="set-link">{set.id}</a>{#if latest && latest.id !== set.id} <span class="tag" data-testid="superseded">earlier set</span>{/if}{/if}
        {:else}
          <span class="sep">·</span>Card {x.card}{#if x.service}<span class="sep">·</span>Service {x.service}{/if}
          {#if x.run}<span class="sep">·</span>{x.run}{/if}
          {#if x.session}<span class="sep">·</span><a href={verifyHref([['session', x.session]])} data-testid="session-link">session {x.session}</a>{/if}
          {#if x.members}<span class="sep">·</span>{x.members.length} member executions{/if}
        {/if}
        <span class="sep">·</span>{x.status === 'pending' ? 'queued' : 'started'} {when(x.at)} UTC
        <span class="sep">·</span>triggered by {trigger}
      </div>
    </div>
    <div class="xside">
      <dl class="xstate">
        <div><dt>Status</dt><dd><span class="badge {life[2]}" data-testid="status"><span aria-hidden="true">{life[0]}</span>{life[1]}</span>
          {#each blocked as b (b)}<span class="tag" data-blocked={b}>{ACTION[b][1]}</span>{/each}</dd></div>
        <div><dt>Verdict</dt><dd data-testid="verdict">
          {#if x.verdict}{@const v = VERDICT[x.verdict]}<span class="badge {v[2]}"><span aria-hidden="true">{v[0]}</span>{v[1]}</span>
          {:else}<span class="muted small">{unfinished ? 'none while running' : 'no recorded verdict'}</span>{/if}
        </dd></div>
        <div><dt>Requirements</dt><dd class="mono small">{judged} of {Object.keys(def.reqs).length} judged</dd></div>
        {#if Object.keys(def.steps).length}
          <div><dt>Outputs</dt><dd><button class="linkbtn small" data-testid="all-outputs-btn" aria-pressed={ui.sel === 'execution:'} onclick={() => select('execution:')}>All retained outputs ({outputCount})</button></dd></div>
        {/if}
      </dl>
      <div class="xactions">
        <button class="btn btn-sm" class:btn-copied={copied} data-testid="copy-link" onclick={copyLink}>{copied ? 'Link copied' : '⧉ Copy link'}</button>
        <a class="btn btn-sm" data-testid="back" href={back}>← {backLabel(back)}</a>
        {#if admin() && cancellable(x)}
          <button class="btn btn-sm btn-danger" data-testid="cancel" disabled={!!ui.busy} onclick={() => cancel(x.id)}>{ui.busy === `${x.id}:cancel` ? 'Cancelling…' : 'Cancel execution'}</button>
        {/if}
      </div>
    </div>
  </div>
  {#if set?.note}<p class="banner info small" data-testid="set-note">{set.note}</p>{/if}
  {#if ui.notice[x.id]}<p class="banner {ui.notice[x.id].kind}" role="status" data-testid="notice">{ui.notice[x.id].text}</p>{/if}
  {#if windows.length}
    <div class="windows" role="group" aria-label="Drift windows">
      {#if wi > 0}<a class="btn btn-sm" data-testid="prev-window" href={toWindow(windows[wi - 1])}>‹ Previous</a>{:else}<span class="btn btn-sm" aria-disabled="true">‹ Previous</span>{/if}
      <span class="mono small" data-testid="window">Window {day(x.window[0])} 00:00 → {day(x.window[1])} 00:00 UTC</span>
      <span class="small muted">{wi + 1} of {windows.length}</span>
      {#if wi < windows.length - 1}<a class="btn btn-sm" data-testid="next-window" href={toWindow(windows[wi + 1])}>Next ›</a>{:else}<span class="btn btn-sm" aria-disabled="true">Next ›</span>{/if}
    </div>
  {/if}
  {#key graphKey}<Graph {mode} />{/key}
  <Inspector />
</div>

{#if fallback}
  <div class="copy-fallback">
    <label class="small muted" for="copy-input">Copy this link</label>
    <input id="copy-input" class="mono" readonly value={fallback}>
    <button class="btn btn-sm" onclick={() => (fallback = '')}>Done</button>
  </div>
{/if}
