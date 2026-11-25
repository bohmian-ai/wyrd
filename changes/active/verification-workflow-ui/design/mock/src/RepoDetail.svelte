<script>
  import CopyBtn from './CopyBtn.svelte';
  import Fact from './Fact.svelte';
  import Lifecycle from './Lifecycle.svelte';
  import { REPOS, CONN, cardYaml, tomlFor } from './catalog.js';
  import { ui, execs, access, readiness, admin, setsOf, repoHref, execHref, pullHref, verifierHref, verifyHref } from './store.svelte.js';

  // One Repository Card at one viewed version. Viewing an older version never changes which version is selected.
  const r = $derived(REPOS.find((x) => x.card === ui.card));
  const ver = $derived(r.versions[ui.v]);
  const isSel = $derived(ui.v === r.selected);
  const a = $derived(access(r));
  const rd = $derived(readiness(r));
  const identity = $derived(`Repository/${r.card}@${ui.v}`);
  const bindingState = $derived(isSel ? (rd.key === 'ready' ? ['●', 'c-ok', 'active'] : ['⊘', 'c-warn', 'inactive · no access']) : ['○', 'c-muted', 'not selected']);

  // Activity comes from recorded review sets: by default the latest set of each pull request, optionally every earlier one.
  const all = $derived(setsOf(r.card));
  const latestIds = $derived(new Set([...new Set(all.map((s) => s.pr))].map((pr) => all.find((s) => s.pr === pr).id)));
  const shown = $derived(all.filter((s) => ui.reviews === 'all' || latestIds.has(s.id)));
  const rows = $derived(shown.flatMap((s) => s.members.map((id) => ({ s, x: execs[id] }))));
  const decl = $derived(ui.dtab === 'yaml' ? cardYaml(r, ui.v) : tomlFor(r, ui.v));
</script>

<div class="xhead">
  <div class="xtitle">
    <div class="xname">
      <span class="kt">Repository</span><h1 class="mono" data-testid="card-name">{r.card}</h1>
      <select class="vsel" aria-label="Card version" data-testid="version" value={ui.v} onchange={(e) => (location.hash = repoHref({ v: e.currentTarget.value }))}>
        {#each Object.keys(r.versions) as x (x)}<option value={x}>{x === r.selected ? `${x} · selected` : x}</option>{/each}
      </select>
      {#if isSel}<span class="badge c-ok" data-testid="sel-marker">✓ selected</span>{:else}<span class="tag" data-testid="sel-marker">not selected · viewing only</span>{/if}
    </div>
    <div class="xmeta"><span data-testid="identity">{identity}</span><span class="sep">·</span>uid {ver.uid}<span class="sep">·</span>registered {ver.registered}</div>
    <p class="desc">{r.desc}</p>
  </div>
  <div class="xactions">
    <CopyBtn label="Copy identity" text={identity} />
    <CopyBtn label="Copy link" text="{location.origin}{location.pathname}{repoHref()}" />
    <a class="btn btn-sm" href="https://{CONN.host}/{r.full}" target="_blank" rel="noopener noreferrer">{r.full} ↗</a>
  </div>
</div>
{#if !isSel}
  <div class="banner info" role="status" data-testid="old-version">You are viewing {ui.v}. Automatic verification uses the selected version {r.selected}; viewing does not change it. To select {ui.v} again, declare it with wyrd declare. <a href={repoHref({ v: r.selected })}>View {r.selected}</a></div>
{/if}
<dl class="facts" data-testid="facts">
  <div><dt>Card</dt><dd><span class="st c-text" data-card="registered"><span aria-hidden="true">●</span>Registered</span><span class="why">{Object.keys(r.versions).length} versions · selected {r.selected}</span></dd></div>
  <div><dt>GitHub access</dt><dd><Fact f={a} name="access" /><span class="why">{a.why}</span><a class="small" href="#/settings/github">{admin() ? 'Manage connection' : 'View connection'}</a></dd></div>
  <div><dt>Automatic verification</dt><dd><Fact f={rd} name="ready" /><span class="why">{rd.why}</span></dd></div>
</dl>
<div class="split">
  <section class="section" aria-labelledby="b-h">
    <div class="section-head"><h2 class="h" id="b-h">verified_by · {identity}</h2></div>
    {#if ver.verifiers.length}
      <div class="outs-wrap">
        <table class="outs">
          <thead><tr><th scope="col">Verifier</th><th scope="col">Exact version</th><th scope="col">Starts on</th><th scope="col">Binding</th><th scope="col">Definition</th></tr></thead>
          <tbody>
            {#each ver.verifiers as [n, x] (n)}
              <tr data-binding="{n}@{x}">
                <td class="cell"><div class="cellbox"><span class="mono">{n}</span></div></td>
                <td class="cell"><div class="cellbox"><span class="mono">{x}</span></div></td>
                <td class="cell"><div class="cellbox"><span class="mono">runs_on: pull_request</span></div></td>
                <td class="cell"><div class="cellbox"><span class="st {bindingState[1]}"><span aria-hidden="true">{bindingState[0]}</span>{bindingState[2]}</span></div></td>
                <td class="cell"><div class="cellbox"><a href={verifierHref(`${n}@${x}`)} data-verifier="{n}@{x}">Verifier definition</a></div></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="note box" data-testid="no-bindings">Version {ui.v} declares verifier = []. Pull requests start no automatic verification; GitHub access is unaffected.</p>
    {/if}
  </section>
  <section class="panel" aria-label="Declaration (read-only)">
    <div class="tabs" role="tablist">
      <a class="tab" role="tab" href={repoHref({ dtab: 'yaml' })} aria-selected={ui.dtab === 'yaml'} data-tab="yaml">Card YAML</a>
      <a class="tab" role="tab" href={repoHref({ dtab: 'toml' })} aria-selected={ui.dtab === 'toml'} data-tab="toml">wyrd.toml</a>
    </div>
    <div class="tabpanel" role="tabpanel">
      <pre class="code small-code" data-testid="decl">{decl}</pre>
      <p class="note small">Read-only. Change policy by editing the declaration and running wyrd declare.</p>
    </div>
  </section>
</div>
<section class="section" aria-labelledby="a-h">
  <div class="section-head">
    <h2 class="h" id="a-h">Verification activity</h2>
    <div class="segs" role="group" aria-label="Review sets">
      <a class="seg" href={repoHref({ reviews: 'latest' })} aria-current={ui.reviews === 'latest' ? 'page' : undefined} data-reviews="latest">Latest per PR</a>
      <a class="seg" href={repoHref({ reviews: 'all' })} aria-current={ui.reviews === 'all' ? 'page' : undefined} data-reviews="all">Include earlier review sets</a>
    </div>
    <a class="btn btn-sm" href={verifyHref([['repo', r.full]], '14d')} data-testid="view-all">View all in Verify</a>
  </div>
  <div class="outs-wrap">
    <table class="outs" data-testid="activity">
      <thead><tr><th scope="col">Execution</th><th scope="col">Pull request</th><th scope="col">Head</th><th scope="col">Verifier</th><th scope="col">Policy</th><th scope="col">Status</th><th scope="col">Verdict</th></tr></thead>
      <tbody>
        {#each rows as { s, x } (x.id)}
          <tr data-exe-row={x.id} data-set={s.id}>
            <td class="cell"><div class="cellbox"><a class="mono" href={execHref(x.id)} data-exe={x.id}>{x.id}</a>
              <span class="sub"><a href={pullHref(r.card, s.pr, s.id)} data-set-link={s.id}>{s.id}</a> · {s.occurrence}{latestIds.has(s.id) ? '' : ' · earlier'}</span></div></td>
            <td class="cell"><div class="cellbox"><span class="prose">{s.title}</span>
              <span class="sub"><a href={pullHref(r.card, s.pr)} data-pr-link={s.pr}>#{s.pr} review sets</a> · <a href="https://{CONN.host}/{r.full}/pull/{s.pr}" target="_blank" rel="noopener noreferrer">GitHub ↗</a></span></div></td>
            <td class="cell"><div class="cellbox"><span class="mono">{s.head}</span></div></td>
            <td class="cell"><div class="cellbox"><span class="mono">{x.def}</span></div></td>
            <td class="cell"><div class="cellbox"><span class="mono">{r.card}@{s.policy}</span></div></td>
            <td class="cell"><div class="cellbox"><Lifecycle {x} part="status" /></div></td>
            <td class="cell"><div class="cellbox"><Lifecycle {x} part="verdict" /></div></td>
          </tr>
        {:else}
          <tr><td colspan="7"><span class="prose muted">No review set recorded for this Card.</span></td></tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="note">Each row is one exact execution pinned to its review set, Repository version, Verifier version and head. A review set is recorded when a pull request opens, gets a new head or has its checks rerequested. Cancelled or completed executions keep their recorded evidence.</p>
</section>
