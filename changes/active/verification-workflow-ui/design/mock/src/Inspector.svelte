<script>
  import Preview from './Preview.svelte';
  import DriftDetail from './DriftDetail.svelte';
  import { ACTION, OUTCOME, DISPOSITION, VERDICT, LIFECYCLE, instKey, instLabel, bytes, sizeOf, when } from './data.js';
  import { ui, execs, select, selection, openOutput, openTrace, execHref, currentInst, reqState, sync, current, decide, failedOf, admin } from './store.svelte.js';

  const def = $derived(current().def);
  const x = $derived(current().exec);
  const sel = $derived(selection());
  const step = $derived(sel.type === 'step' ? { id: sel.id, def: def.steps[sel.id], st: x?.steps[sel.id] } : null);
  const inst = $derived(step?.st ? currentInst(step.id) : null);
  const out = $derived(inst?.outs.find((o) => o.name === ui.out) ?? inst?.outs[0]);
  // Simple steps run once: no item, iteration or attempt controls.
  const multi = $derived(!!step?.st && (!!step.def.fanout || !!step.def.repeat || step.st.inst.length > 1));
  const tabs = $derived(step?.st ? ['overview', ...(step.def.reqs.length ? ['requirements'] : []), 'outputs'] : []);
  const tab = $derived(tabs.includes(ui.tab) ? ui.tab : 'overview');
  const span = $derived(ui.trace && x?.spans?.find((s) => s.id === ui.trace));

  // Every retained publication, in definition order. None is privileged: a final step's output is one row like any other.
  const allOutputs = $derived(x ? Object.keys(def.steps).flatMap((s) => x.steps[s].inst.flatMap((i) => i.outs.map((o) => ({ step: s, i, o })))) : []);

  const setTab = (t) => { ui.tab = t; sync(); };
  const setInst = (i) => { ui.inst = instKey(i); ui.out = i.outs[0]?.name ?? ''; sync(); };
  const setOut = (name) => { ui.out = name; ui.tab = 'outputs'; sync(); };
  const placement = (r) => (def.reqs[r].step ? `in step ${def.reqs[r].step}` : `after all steps · reads ${def.reqs[r].reads}`);
  const readBy = (id) => [...Object.keys(def.steps).filter((s) => def.steps[s].deps.includes(id)), ...def.after.filter((r) => def.reqs[r].reads === id)].join(', ');
  const pick = (v, pointer) => pointer.split('/').filter(Boolean).reduce((o, k) => o?.[k], v);

  let reason = $state('');
  const APPROVAL = { pending: ['◷', 'pending', 'c-warn'], approved: ['✓', 'approved', 'c-ok'], rejected: ['✕', 'rejected', 'c-danger'],
    expired: ['⧗', 'expired', 'c-muted'], cancelled: ['⊘', 'closed by cancellation', 'c-muted'] };
</script>

{#snippet cites(list)}
  <div class="finals"><span class="h">Evidence</span>
    {#each list as c, k (k)}
      {#if Array.isArray(c)}
        {@const ci = x.steps[c[0]].inst.find((i) => instKey(i) === c[1])}
        <button class="cite" data-cite={c[2]} onclick={() => openOutput(c[0], c[1], c[2])}>{c[0]} · {instLabel(ci)} · {c[2]}</button>
      {:else if c.span}
        <button class="cite" data-cite={c.span} onclick={() => openTrace(c.span)}>trace {x.inputs.trace.value} · span {x.spans.find((s) => s.id === c.span).name}</button>
      {:else}
        <a class="cite" data-cite={c.exe} href={execHref(c.exe, { sel: `input:${c.input}`, cite: c.pointer })}>{execs[c.exe].run} · {c.input} {c.pointer}</a>
      {/if}
    {/each}
  </div>
{/snippet}

{#snippet reqDetail(r)}
  {@const q = def.reqs[r]}
  {@const res = reqState(r)}
  {@const o = OUTCOME[res.outcome]}
  <div class="reqd {x ? res.outcome : ''}" data-testid="req-detail">
    <div class="gn-row"><span class="gn-id">{r}</span><span class="kt">{q.kind}</span>{#if x}<span class="badge {o[2]}"><span aria-hidden="true">{o[0]}</span>{o[1]}</span>{/if}</div>
    <dl class="dkv">
      <dt>Expects</dt><dd class="prose">{q.desc}</dd>
      <dt>Check</dt><dd>{q.check}</dd>
      <dt>Judged</dt><dd>{placement(r)}</dd>
      {#if q.baseline}<dt>Baseline</dt><dd>{q.baseline} · {q.method}</dd>{/if}
      {#if x && res.outcome !== 'none'}
        <dt>Reason</dt><dd class="prose">{res.reason}</dd>
        <dt>Actual</dt><dd>{res.actual}</dd>
        {#if res.judge}<dt>Judge</dt><dd>{res.judge}</dd>{/if}
      {/if}
    </dl>
    {#if x && res.outcome === 'none'}
      <p class="note">Not judged yet. {q.step ? `Judged when ${q.step} settles.` : x.status === 'cancelled' ? 'The execution was cancelled before every step settled.' : 'Judged after every step has settled.'}</p>
    {/if}
    {#if x && res.drift}<DriftDetail {x} req={r} />{/if}
    {#if x && res.cites}{@render cites(res.cites)}{/if}
  </div>
{/snippet}

{#snippet approval(a)}
  {@const s = APPROVAL[a.state]}
  {@const busy = ui.busy === `${x.id}:decide`}
  <div class="approval" data-testid="approval">
    <div class="gn-row"><span class="h">Human approval</span><span class="badge {s[2]}" data-testid="approval-state"><span aria-hidden="true">{s[0]}</span>{s[1]}</span></div>
    <p class="prose">{a.message}</p>
    <dl class="dkv">
      <dt>Occurrence</dt><dd data-testid="occurrence">{a.occurrence}</dd>
      <dt>For</dt><dd>{x.id} · step {step.id} · {instLabel(inst)}</dd>
      <dt>Requested</dt><dd>{when(a.requested)} UTC</dd>
      <dt>Expires</dt><dd>{when(a.expires)} UTC</dd>
      {#if a.by}<dt>Decided</dt><dd>{a.state} by {a.by} at {a.at}{#if a.reason} · “{a.reason}”{/if}</dd><dt>Receipt</dt><dd>{a.receipt}</dd>{/if}
    </dl>
    {#if a.state === 'pending'}
      {#if admin()}
        <label class="field small muted">Reason (optional)
          <textarea rows="2" bind:value={reason} disabled={busy} data-testid="decision-reason"></textarea>
        </label>
        <div class="xactions">
          <button class="btn btn-primary" data-testid="approve" disabled={busy} onclick={() => decide(x.id, step.id, 'approved', reason)}>Approve</button>
          <button class="btn" data-testid="reject" disabled={busy} onclick={() => decide(x.id, step.id, 'rejected', reason)}>Reject</button>
          {#if busy}<span class="small muted" role="status">Submitting for {a.occurrence}…</span>{/if}
        </div>
      {:else}
        <p class="note" data-testid="viewer-note">You can read this approval. Approving or rejecting needs the approve scope, which the preview viewer role does not have.</p>
      {/if}
    {/if}
  </div>
{/snippet}

<section class="inspector" aria-label="Inspector" data-testid="inspector">
  {#if span}
    <div class="insp-head">
      <button class="btn btn-sm" data-testid="trace-back" onclick={() => { ui.trace = ''; sync(); }}>← Back to {sel.id}</button>
      <span class="gn-id" data-testid="insp-title">span {span.name}</span><span class="tag">sample evidence</span>
      <span class="small muted">trace {x.inputs.trace.value} · {span.id}</span>
    </div>
    <div class="tabpanel" data-testid="trace-panel">
      <p class="note">A bounded sample of the cited trace: the spans this execution's trace checks read. It is not a trace explorer.</p>
      <div class="outs-wrap">
        <table class="outs">
          <thead><tr><th>Span</th><th>Start</th><th>Duration</th><th>Attributes</th></tr></thead>
          <tbody>
            {#each x.spans as s (s.id)}
              <tr aria-selected={s.id === span.id}><td><span style="padding-left: {10 + s.depth * 14}px">{s.name} <span class="muted">{s.id}</span></span></td><td><span>+{s.start} ms</span></td><td><span>{s.dur} ms</span></td>
                <td><span>{Object.entries(s.attrs).map(([k, v]) => `${k}=${v}`).join('  ')}</span></td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {:else if step && !x}
    <div class="insp-head">
      <span class="gn-id" data-testid="insp-title">{step.id}</span><span class="kt">{step.def.kind}</span><span class="mono small muted">{step.def.ref}</span>
      <p class="desc">{step.def.desc}</p>
    </div>
    <div class="tabpanel">
      <dl class="dkv">
        <dt>Depends on</dt><dd>{step.def.deps.join(', ')}</dd>
        {#if step.def.when}<dt>Condition</dt><dd>when {step.def.when}</dd>{/if}
        {#if step.def.fanout}<dt>Fan-out</dt><dd>one invocation per item of {step.def.fanout}</dd>{/if}
        {#if step.def.repeat}<dt>Repeat</dt><dd>until {step.def.repeat.until}, at most {step.def.repeat.max} iterations</dd>{/if}
        <dt>Requirements</dt><dd>{step.def.reqs.join(', ') || 'none attached'}</dd>
      </dl>
    </div>
  {:else if step}
    {@const a = ACTION[step.st.action]}
    <div class="insp-head">
      <span class="gn-id" data-testid="insp-title">{step.id}</span><span class="kt">{step.def.kind}</span>
      <span class="mono small muted">{step.def.ref}</span>
      <span class="right">
        <span class="badge {a[2]}" title="Action completion"><span aria-hidden="true">{a[0]}</span>{a[1]}</span>
        {#if step.def.when}<span class="tag" title="Speculative disposition">disposition <b>{DISPOSITION[step.st.disposition][0]}</b></span>{/if}
      </span>
      <p class="desc">{step.def.desc}</p>
    </div>
    <div class="insp-body" class:single={!multi}>
      {#if multi}
        <div class="insp-side">
          <span class="h">Invocations · {step.st.inst.length}</span>
          <div role="radiogroup" aria-label="Invocations" class="insts">
            {#each step.st.inst as i (instKey(i))}
              {@const ia = ACTION[i.state]}
              <button class="inst" role="radio" aria-checked={instKey(i) === instKey(inst)} data-inst={instKey(i)} onclick={() => setInst(i)}>
                <span>{instLabel(i)}</span><span class={ia[2]}><span aria-hidden="true">{ia[0]}</span> {ia[1]}</span>
                {#if i.reason}<span class="why">{i.reason}</span>{/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}
      <div class="insp-main">
        <div class="tabs" role="tablist" aria-label="Step detail">
          {#each tabs as t (t)}
            <button class="tab" role="tab" aria-selected={t === tab} onclick={() => setTab(t)}>
              {t[0].toUpperCase() + t.slice(1)}{#if t === 'outputs' && inst} · {inst.outs.length}{/if}
            </button>
          {/each}
        </div>
        <div class="tabpanel" role="tabpanel">
          {#if tab === 'overview'}
            {#if step.st.approval}{@render approval(step.st.approval)}{/if}
            {#if step.st.waitingFor}
              <p class="note box waitnote" data-testid="waiting-input">Waiting for input <b class="mono">{step.st.waitingFor.join(', ')}</b>. The execution started without it; this step starts when the evidence is provided. Nothing here is judged until then.</p>
            {/if}
            <dl class="dkv">
              <dt>Action</dt><dd>{a[1]}{#if step.st.reason} · {step.st.reason}{/if}</dd>
              <dt>Depends on</dt><dd>{step.def.deps.join(', ')}</dd>
              {#if step.def.when}
                <dt>Condition</dt><dd>when {step.def.when}</dd>
                <dt>Disposition</dt><dd class="prose">{DISPOSITION[step.st.disposition][0]}: {step.st.reason ?? DISPOSITION[step.st.disposition][1]}</dd>
              {/if}
              {#if step.def.fanout}<dt>Fan-out</dt><dd>one invocation per item of {step.def.fanout}</dd>{/if}
              {#if step.def.repeat}<dt>Repeat</dt><dd>until {step.def.repeat.until}, at most {step.def.repeat.max} iterations</dd>{/if}
              {#if inst}<dt>{multi ? 'Selected' : 'Invocation'}</dt><dd>{instLabel(inst)} · {ACTION[inst.state][1]} · {inst.outs.length} {inst.outs.length === 1 ? 'publication' : 'publications'}{#if !multi && inst.reason} · {inst.reason}{/if}</dd>
              {:else if !step.st.waitingFor}<dt>Invocation</dt><dd>none yet · starts once {step.def.deps.join(' and ')} {step.def.deps.length > 1 ? 'have' : 'has'} settled</dd>{/if}
            </dl>
          {:else if tab === 'requirements'}
            {#each step.def.reqs as r (r)}{@render reqDetail(r)}{/each}
          {:else if inst && inst.outs.length}
            <div class="outs-wrap">
              <table class="outs" aria-label="Publications of the selected invocation">
                <thead><tr><th>Name</th><th>Kind</th><th>Size</th></tr></thead>
                <tbody>
                  {#each inst.outs as o (o.name)}
                    <tr aria-selected={o === out}><td><button class="rowbtn" data-out={o.name} onclick={() => setOut(o.name)}>{o.name}{o.kind === 'directory' ? '/' : ''}</button></td><td><span>{o.kind}</span></td><td><span>{bytes(sizeOf(o))}</span></td></tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <Preview out={out} producer="{step.id} · {instLabel(inst)}" live={inst.state === 'started' && x.status === 'running'} />
          {:else}
            <p class="note">{inst ? 'This invocation has not published anything.' : 'No invocation yet, so nothing is published.'}</p>
          {/if}
        </div>
      </div>
    </div>
  {:else if sel.type === 'req'}
    <div class="insp-head"><span class="gn-id" data-testid="insp-title">{sel.id}</span><span class="small muted">Requirement · {placement(sel.id)}</span></div>
    <div class="tabpanel">
      {@render reqDetail(sel.id)}
      {#if x?.inputs}
        <details class="inputs" data-testid="inputs">
          <summary class="h">Inputs · {Object.keys(x.inputs).join(' · ')}</summary>
          <dl class="dkv">
            {#each Object.entries(x.inputs) as [k, v] (k)}
              <dt>{k}</dt><dd>{#if v.awaited}<span class="c-warn">not provided yet</span>{:else if k === 'trace'}<button class="cite" onclick={() => openTrace(x.spans[0].id)}>{v.value} · {x.spans.length} spans ↗</button>{:else}{typeof v.value === 'string' ? v.value : JSON.stringify(v.value)}{/if}</dd>
            {/each}
          </dl>
        </details>
      {/if}
    </div>
  {:else if sel.type === 'input'}
    {@const v = x?.inputs?.[sel.id]}
    <div class="insp-head"><span class="gn-id" data-testid="insp-title">{sel.id}</span><span class="tag">{def.inputs[sel.id].kind}</span><span class="small muted">{x ? 'Execution input' : 'Declared input'}</span></div>
    <div class="tabpanel">
      {#if v?.awaited}<p class="note box waitnote" data-testid="waiting-input">{v.awaited}</p>{/if}
      {#if ui.cite && v}
        <p class="note box" data-testid="cited">Cited by the session judgment at <span class="mono">{sel.id} {ui.cite}</span>: “{pick(v.value, ui.cite)}”</p>
      {/if}
      <dl class="dkv">
        <dt>Contains</dt><dd class="prose">{def.inputs[sel.id].sub}</dd>
        <dt>Read by</dt><dd>{readBy(sel.id)}</dd>
        {#if x?.pr}<dt>Bound for</dt><dd>{x.repo} #{x.pr} at {x.head}</dd>{/if}
        {#if v && !v.awaited}<dt>Value</dt><dd>{typeof v.value === 'string' ? v.value : JSON.stringify(v.value)}</dd>{/if}
      </dl>
    </div>
  {:else if sel.type === 'member'}
    {@const m = execs[sel.id]}
    <div class="insp-head"><span class="gn-id" data-testid="insp-title">{m.run ?? m.id}</span><span class="tag">member execution</span><span class="small muted">pinned when this session was judged</span></div>
    <div class="tabpanel">
      <dl class="dkv">
        <dt>Execution</dt><dd>{m.id}</dd>
        <dt>Verifier</dt><dd>{m.def}</dd>
        <dt>Status</dt><dd>{LIFECYCLE[m.status][1]}</dd>
        <dt>Verdict</dt><dd>{m.verdict ? VERDICT[m.verdict][1] : 'no recorded verdict'}</dd>
        <dt>Failed</dt><dd>{failedOf(m).join(', ') || 'none'}</dd>
        <dt>Recorded</dt><dd>{when(m.at)} UTC</dd>
      </dl>
      <p style="margin: 12px 0 0"><a class="btn btn-sm" data-testid="open-member" href={execHref(m.id)}>Open {m.id}</a></p>
    </div>
  {:else if x}
    <div class="insp-head"><span class="gn-id" data-testid="insp-title">All retained outputs</span><span class="small muted">{allOutputs.length} publications across every step and invocation. Selecting one opens its producer.</span></div>
    <div class="tabpanel">
      {#if allOutputs.length}
        <div class="outs-wrap">
          <table class="outs" data-testid="all-outputs">
            <thead><tr><th>Name</th><th>Producer</th><th>Invocation</th><th>Kind</th><th>Size</th></tr></thead>
            <tbody>
              {#each allOutputs as { step: s, i, o } (s + instKey(i) + o.name)}
                <tr><td><button class="rowbtn" data-producer="{s}|{instKey(i)}|{o.name}" onclick={() => openOutput(s, instKey(i), o.name)}>{o.name}{o.kind === 'directory' ? '/' : ''}</button></td>
                  <td><span>{s}</span></td><td><span>{instLabel(i)} · <span class={ACTION[i.state][2]}>{ACTION[i.state][1]}</span></span></td><td><span>{o.kind}</span></td><td><span>{bytes(sizeOf(o))}</span></td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="note">No step has published anything in this execution.</p>
      {/if}
    </div>
  {:else}
    <div class="insp-head"><span class="gn-id" data-testid="insp-title">{def.key}</span><span class="small muted">{Object.keys(def.inputs).length} inputs · {Object.keys(def.steps).length} steps · {Object.keys(def.reqs).length} requirements</span></div>
    <div class="tabpanel"><p class="note">Select a node to read its declaration. Execution state appears only when you open an execution in Verify.</p></div>
  {/if}
</section>
