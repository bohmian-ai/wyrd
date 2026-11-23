<!-- #/platform/ui/*: the platform console. Rust owns the session (Path=/platform cookie); pages render its safe
     metadata. Destructive changes need fresh proof from the same operator, asked for in place. -->
<script>
  import logo from '../../../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/logo.svg';
  import Problem from './Problem.svelte';
  import { P, PROVIDERS, TENANTS, SOCIAL, RELAY, LOGIN_SETTINGS, platformSession } from './data.js';
  let { view, query } = $props();
  // App re-creates the page on every hash change, so reading the props once is the whole lifetime.
  const q = (key) => query.get(key);
  const preview = q('preview');
  const VIEWS = [['', 'Overview'], ['providers', 'Social sign-in'], ['mail', 'Mail relay'], ['signup', 'Signup']];
  if (q('method') === 'oidc') platformSession.method = 'oidc';
  if (preview?.startsWith('reauth')) platformSession.recent = false;
  const session = $state({ ...platformSession });

  let issue = $state.raw(null);
  let notice = $state('');
  let pending = $state.raw(preview?.startsWith('reauth') ? { label: 'Suspending acme-research', apply: () => suspend('acme-research', 'suspended') } : null);
  let reauthIssue = $state.raw(preview === 'reauth-refused' ? P.reauthRefused : null);
  let reauthPassword = $state(preview === 'reauth-refused' ? 'not-my-password' : '');
  let confirming = $state(false);

  /** Run a destructive change now, or ask for fresh proof first (the server answers 403 RECENT_LOGIN_REQUIRED otherwise). */
  function attempt(label, apply) {
    notice = '';
    issue = null;
    if (session.recent) apply();
    else { pending = { label, apply }; reauthIssue = null; reauthPassword = ''; }
  }
  function proven() {
    session.recent = platformSession.recent = true;
    confirming = false;
    const { apply } = pending;
    pending = null;
    apply();
  }
  // Simulates POST /platform/browser/password/reauth {password}: 200 with recent: true, same cookie.
  function reauthPasswordSubmit(event) {
    event.preventDefault();
    if (!reauthPassword) return document.getElementById('reauth-password').focus();
    confirming = true;
    setTimeout(() => {
      if (reauthPassword === 'not-my-password' || reauthPassword === 'wrong') { reauthIssue = P.reauthRefused; confirming = false; return; }
      proven();
    }, 700);
  }
  // Simulates POST /platform/browser/oidc/reauth (prompt=login, max_age=0), the IdP and the console callback.
  function reauthSso() {
    confirming = true;
    setTimeout(proven, 1000);
  }
  // Simulates POST /platform/browser/logout: 204, only this session.
  function signOut() {
    Object.assign(platformSession, { recent: false });
    location.hash = '#/platform?preview=ended';
  }

  // Overview: GET /platform/tenants; PUT /platform/tenants/{id}/status {status: "active" | "suspended"}.
  let tenants = $state(TENANTS.map((t) => ({ ...t })));
  function suspend(slug, status) {
    tenants.find((t) => t.slug === slug).status = status;
    notice = `${slug} is ${status}.`;
  }
  const STATUS = { active: '✓ active', suspended: '⊘ suspended', provisioning: '◌ provisioning', failed: '✕ failed' };

  // Social sign-in: GET /platform/social/providers; PUT …/{provider}; POST …/{provider}/test; PUT …/{provider}/enabled.
  let providers = $state(SOCIAL.map((p) => ({ ...p, draftId: p.client_id ?? '', draftSecret: '', testing: false })));
  const STATE = { untested: '◌ untested · not offered', tested: '◐ tested · not offered', enabled: '✓ enabled · offered' };
  function saveClient(p) {
    if (!p.draftId.trim() || !p.draftSecret) { issue = { status: 400, code: 'WYRD_SPEC_400_VALIDATION', title: 'Enter a client ID and secret', remediation: 'Both come from the provider console. The secret is stored sealed and never shown again.' }; return; }
    attempt(`Replacing the ${PROVIDERS[p.provider]} client`, () => {
      Object.assign(p, { client_id: p.draftId.trim(), revision: (p.revision ?? 0) + 1, state: 'untested', draftSecret: '' });
      notice = `${PROVIDERS[p.provider]} revision ${p.revision} saved. It is untested and not offered.`;
    });
  }
  function test(p) {
    notice = ''; issue = null;
    if (preview === 'conflict') { issue = P.providerConflict; return; }
    p.testing = true;
    setTimeout(() => {
      p.testing = false;
      if (p.state === 'untested') p.state = 'tested';
      notice = `Signed in at ${PROVIDERS[p.provider]}. Revision ${p.revision} is tested${p.state === 'tested' ? '; enable it to offer it' : ''}.`;
    }, 1000);
  }
  function toggle(p) {
    const on = p.state !== 'enabled';
    attempt(`${on ? 'Enabling' : 'Disabling'} ${PROVIDERS[p.provider]}`, () => {
      p.state = on ? 'enabled' : 'tested';
      notice = `${PROVIDERS[p.provider]} is ${on ? 'offered on sign-in and signup pages' : 'no longer offered'}.`;
    });
  }

  // Mail relay: GET/PUT /platform/mail; POST /platform/mail/test {recipient} -> {accepted, error}.
  const stored = preview === 'none' ? null : RELAY;
  let relay = $state(stored && { ...stored });
  const draft = $state({ host: stored?.host ?? '', port: stored?.port ?? 587, tls: stored?.tls ?? 'starttls', username: stored?.username ?? '', password: '', sender: stored?.sender ?? '' });
  let recipient = $state('ops@example.test');
  let testResult = $state.raw(preview === 'test-failed' ? { accepted: false, error: '535 5.7.8 Authentication credentials invalid' } : null);
  let testing = $state(false);
  function saveRelay(event) {
    event.preventDefault();
    if (!draft.host.trim() || !draft.sender.includes('@') || (draft.password && !draft.username.trim())) {
      issue = { status: 400, code: 'WYRD_SPEC_400_VALIDATION', title: 'Check the relay settings', remediation: 'Enter a host and a sender address. A password needs a username.' };
      return;
    }
    attempt('Replacing the mail relay', () => {
      relay = { host: draft.host.trim(), port: draft.port, tls: draft.tls, username: draft.username.trim() || null, password_set: !!draft.password, sender: draft.sender.trim() };
      draft.password = '';
      notice = 'Mail relay saved.';
    });
  }
  function sendTest(event) {
    event.preventDefault();
    testing = true;
    testResult = null;
    setTimeout(() => { testing = false; testResult = { accepted: true, error: null }; }, 800);
  }

  // Signup: GET/PUT /platform/login-settings. The challenge secret is write-only; omitting it keeps the stored one.
  const settings = $state({ ...LOGIN_SETTINGS, site_key: LOGIN_SETTINGS.signup_challenge?.site_key ?? '', secret: '', challenge: !!LOGIN_SETTINGS.signup_challenge });
  function saveSettings(event) {
    event.preventDefault();
    if (!(settings.signup_workspace_cap >= 1)) { issue = { status: 400, code: 'WYRD_SPEC_400_VALIDATION', title: 'The workspace limit must be at least 1', remediation: 'Each proven identity may own this many unretired signup-created workspaces.' }; return; }
    attempt('Changing login and signup settings', () => { settings.secret = ''; notice = 'Login and signup settings saved.'; });
  }
</script>

<div class="console">
  <header>
    <span class="brand"><img src={logo} alt="" width="22" height="22" /><span class="wordmark">Wyrd</span><span class="plane">Platform admin</span></span>
    <nav aria-label="Console">
      {#each VIEWS as [path, label] (path)}
        <a href="#/platform/ui{path ? `/${path}` : ''}" aria-current={view === path ? 'page' : undefined}>{label}</a>
      {/each}
    </nav>
    <span class="who"><span data-testid="operator">{session.name}</span><button class="app-control" type="button" onclick={signOut}>Sign out</button></span>
  </header>

  <main class="body">
    <h1>{VIEWS.find(([p]) => p === view)[1]}</h1>

    {#if pending}
      <div class="reauth" role="dialog" aria-labelledby="reauth-title" data-testid="reauth">
        <h2 id="reauth-title">Confirm it is you</h2>
        <p><b>{pending.label}</b> is a destructive platform change. Confirm your own sign-in again; it stays fresh for five minutes. Nothing changes until you do.</p>
        {#if reauthIssue}<Problem issue={reauthIssue} />{/if}
        {#if session.method === 'password'}
          <form class="stack" novalidate onsubmit={reauthPasswordSubmit}>
            <label for="reauth-password">Your password</label>
            <input id="reauth-password" class="app-input" type="password" autocomplete="current-password" readonly={confirming} bind:value={reauthPassword} />
            <div class="row">
              <button class="app-control primary" type="submit" disabled={confirming} aria-busy={confirming ? 'true' : undefined} data-testid="reauth-confirm">{confirming ? 'Confirming…' : 'Confirm'}</button>
              <button class="app-control" type="button" onclick={() => (pending = null)}>Cancel</button>
            </div>
          </form>
        {:else}
          <p>Your identity provider asks you to sign in again, even if you are signed in there.</p>
          <div class="row">
            <button class="app-control primary" type="button" onclick={reauthSso} disabled={confirming} aria-busy={confirming ? 'true' : undefined} data-testid="reauth-sso">{confirming ? 'Waiting for platform SSO…' : 'Continue with platform SSO'}</button>
            <button class="app-control" type="button" onclick={() => (pending = null)}>Cancel</button>
          </div>
        {/if}
      </div>
    {/if}

    {#if issue}<Problem {issue} />{/if}
    <div role="status" aria-live="polite">{#if notice}<p class="done" data-testid="notice"><span class="ok">✓</span> {notice}</p>{/if}</div>

    {#if view === ''}
      <section aria-labelledby="session">
        <h2 id="session">This session</h2>
        <dl>
          <dt>Operator</dt><dd>{session.name}</dd>
          <dt>Signed in with</dt><dd>{session.method === 'password' ? 'Platform password' : 'Platform SSO'}</dd>
          <dt>Idle timeout</dt><dd>{session.idle_expires_at}</dd>
          <dt>Session ends</dt><dd>{session.expires_at}</dd>
          <dt>Fresh sign-in</dt><dd data-testid="recent">{session.recent ? '✓ yes · changes apply at once' : '– no · changes ask you to confirm'}</dd>
        </dl>
      </section>
      <section aria-labelledby="workspaces">
        <h2 id="workspaces">Workspaces</h2>
        <div class="scroll" role="region" aria-label="Workspaces" tabindex="-1">
          <table>
            <thead><tr><th>Workspace</th><th>Key</th><th>Status</th><th></th></tr></thead>
            <tbody>
              {#each tenants as t (t.slug)}
                <tr>
                  <td>{t.display_name}</td>
                  <td><code>{t.slug}</code></td>
                  <td data-testid="status-{t.slug}">{STATUS[t.status] ?? t.status}</td>
                  <td>
                    {#if t.status === 'active'}
                      <button class="app-control" type="button" onclick={() => attempt(`Suspending ${t.slug}`, () => suspend(t.slug, 'suspended'))} data-testid="suspend-{t.slug}">Suspend</button>
                    {:else if t.status === 'suspended'}
                      <button class="app-control" type="button" onclick={() => attempt(`Resuming ${t.slug}`, () => suspend(t.slug, 'active'))}>Resume</button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <p>Suspending keeps every row, grant and credential; resuming restores them. Provisioning and failed workspaces change only by provisioning.</p>
      </section>
    {:else if view === 'providers'}
      <p class="lead">Shared Google and GitHub.com sign-in for every workspace. Register the client in the provider console first. A saved client is a new, untested revision that no page offers until a real sign-in tests it and you enable it.</p>
      {#each providers as p (p.provider)}
        <section aria-labelledby="provider-{p.provider}" data-testid="provider-{p.provider}">
          <h2 id="provider-{p.provider}">{PROVIDERS[p.provider]} <span class="state" data-testid="state-{p.provider}">{p.state ? STATE[p.state] : '– not configured'}</span></h2>
          <dl>
            <dt>Callback URL</dt><dd><code>{p.callback_url}</code></dd>
            <dt>Client ID</dt><dd><code>{p.client_id ?? '—'}</code></dd>
            <dt>Revision</dt><dd>{p.revision ?? '—'}</dd>
          </dl>
          <p>{p.registration_instructions}</p>
          <form class="stack" novalidate onsubmit={(e) => { e.preventDefault(); saveClient(p); }}>
            <label for="client-id-{p.provider}">Client ID</label>
            <input id="client-id-{p.provider}" class="app-input mono" autocomplete="off" spellcheck="false" bind:value={p.draftId} />
            <label for="client-secret-{p.provider}">Client secret</label>
            <input id="client-secret-{p.provider}" class="app-input mono" type="password" autocomplete="off" bind:value={p.draftSecret} />
            <p>{p.client_id ? 'The stored secret is never shown. Saving replaces the client and starts a new untested revision.' : 'Stored sealed and never shown again.'}</p>
            <div class="row">
              <button class="app-control" type="submit">Save client</button>
              {#if p.state}
                <button class="app-control" type="button" onclick={() => test(p)} disabled={p.testing} aria-busy={p.testing ? 'true' : undefined} data-testid="test-{p.provider}">{p.testing ? `Signing in at ${PROVIDERS[p.provider]}…` : `Test sign-in`}</button>
                <button class="app-control primary" type="button" onclick={() => toggle(p)} disabled={p.state === 'untested'} aria-describedby={p.state === 'untested' ? `why-${p.provider}` : undefined} data-testid="toggle-{p.provider}">{p.state === 'enabled' ? 'Disable' : 'Enable'}</button>
              {/if}
            </div>
            {#if p.state === 'untested'}<p id="why-{p.provider}">Test revision {p.revision} before enabling it.</p>{/if}
          </form>
        </section>
      {/each}
    {:else if view === 'mail'}
      <section aria-labelledby="relay">
        <h2 id="relay">SMTP relay</h2>
        {#if relay}
          <dl data-testid="relay">
            <dt>Host</dt><dd><code>{relay.host}:{relay.port}</code></dd>
            <dt>Security</dt><dd>{relay.tls === 'starttls' ? 'STARTTLS' : relay.tls === 'tls' ? 'TLS (SMTPS)' : 'None'}</dd>
            <dt>Username</dt><dd>{relay.username ?? '—'}</dd>
            <dt>Password</dt><dd>{relay.password_set ? '✓ stored, never shown' : '– none'}</dd>
            <dt>Sender</dt><dd>{relay.sender}</dd>
          </dl>
        {:else}
          <p class="absent" data-testid="no-relay">– No relay. Wyrd emails nothing: invitations are copied links, and email signup, finding workspaces by email, recovery email and reset requests are not offered.</p>
        {/if}
        <form class="stack" novalidate onsubmit={saveRelay}>
          <label for="host">Host</label><input id="host" class="app-input mono" autocomplete="off" bind:value={draft.host} />
          <label for="port">Port</label><input id="port" class="app-input mono" type="number" min="1" max="65535" bind:value={draft.port} />
          <label for="tls">Security</label>
          <select id="tls" class="app-input" bind:value={draft.tls}>
            <option value="starttls">STARTTLS (usually port 587)</option>
            <option value="tls">TLS from the first byte (usually port 465)</option>
            <option value="none">None (trusted local network only)</option>
          </select>
          <label for="relay-user">Username</label><input id="relay-user" class="app-input mono" autocomplete="off" bind:value={draft.username} />
          <label for="relay-password">Password</label><input id="relay-password" class="app-input mono" type="password" autocomplete="off" bind:value={draft.password} />
          <p>Saving replaces the whole relay. The stored password is never shown; enter it again to keep one.</p>
          <label for="sender">Sender address</label><input id="sender" class="app-input" type="email" autocomplete="off" bind:value={draft.sender} />
          <button class="app-control primary" type="submit">Save relay</button>
        </form>
      </section>
      {#if relay}
        <section aria-labelledby="test">
          <h2 id="test">Send a test message</h2>
          <form class="stack" novalidate onsubmit={sendTest}>
            <label for="recipient">Recipient</label><input id="recipient" class="app-input" type="email" bind:value={recipient} />
            <button class="app-control" type="submit" disabled={testing} aria-busy={testing ? 'true' : undefined} data-testid="mail-test">{testing ? 'Sending…' : 'Send test'}</button>
          </form>
          <div role="status">
            {#if testResult?.accepted}
              <p class="done" data-testid="test-result"><span class="ok">✓</span> The relay accepted the message. That does not prove it reached the inbox.</p>
            {:else if testResult}
              <p class="done" data-testid="test-result"><span class="bad">✕</span> The relay did not accept the message. <code>{testResult.error}</code></p>
            {/if}
          </div>
        </section>
      {/if}
    {:else}
      <section aria-labelledby="login">
        <h2 id="login">Login and signup</h2>
        <form class="stack" novalidate onsubmit={saveSettings}>
          <label class="check"><input type="checkbox" bind:checked={settings.password_enabled} /> Password sign-in for workspaces</label>
          <label class="check"><input type="checkbox" bind:checked={settings.signup_enabled} data-testid="signup-enabled" /> Public workspace signup</label>
          <p>Pages offer signup only while a creation method is usable: email signup also needs password sign-in and the mail relay; provider signup needs an enabled provider.</p>
          <label for="cap">Workspaces one identity may create</label>
          <input id="cap" class="app-input mono" type="number" min="1" bind:value={settings.signup_workspace_cap} />
          <p>Counts every signup-created workspace that is not retired, including failed ones. Operator-created workspaces never count.</p>
          <fieldset>
            <legend>Human challenge (Cloudflare Turnstile)</legend>
            <label class="check"><input type="checkbox" bind:checked={settings.challenge} /> Require a challenge for browser workspace creation</label>
            {#if settings.challenge}
              <label for="site-key">Site key</label><input id="site-key" class="app-input mono" autocomplete="off" bind:value={settings.site_key} />
              <label for="site-secret">Secret key</label><input id="site-secret" class="app-input mono" type="password" autocomplete="off" bind:value={settings.secret} />
              <p>The stored secret is never shown. Leave it blank to keep it.</p>
            {/if}
          </fieldset>
          <button class="app-control primary" type="submit" data-testid="save-settings">Save settings</button>
        </form>
      </section>
    {/if}
  </main>
</div>

<style>
  .console {
    min-height: calc(100dvh - 33px);
    background: var(--bg);
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 20px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--border);
    border-top: 4px solid var(--declare);
    background: var(--surface);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .brand img {
    border-radius: var(--r);
  }
  .wordmark {
    font: 600 16px var(--font-sans);
  }
  .plane {
    font: var(--weight-strong) 10px var(--font-mono);
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 3px 10px;
    background: var(--declare);
    color: var(--declare-ink);
  }
  header nav {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    flex: 1;
    font: 13px var(--font-sans);
  }
  header nav a {
    color: var(--muted);
    text-decoration: none;
    padding: 4px 0;
    border-bottom: 2px solid transparent;
  }
  header nav a[aria-current='page'] {
    color: var(--text);
    border-bottom-color: var(--declare);
  }
  .who {
    display: flex;
    align-items: center;
    gap: 10px;
    font: 12px var(--font-mono);
    color: var(--muted);
  }
  .body {
    display: grid;
    gap: 20px;
    align-content: start;
    max-width: 960px;
    padding: 24px;
  }
  h1 {
    font: var(--weight-strong) 24px var(--font-sans);
  }
  section,
  .reauth {
    display: grid;
    gap: 12px;
    min-width: 0;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .reauth {
    border: 2px solid var(--warn);
  }
  h2 {
    margin: 0;
    font: var(--weight-strong) 13px var(--font-sans);
  }
  .state {
    margin-left: 8px;
    font: 11px var(--font-mono);
    color: var(--muted);
  }
  p {
    font: 12px/18px var(--font-sans);
    color: var(--muted);
  }
  .lead {
    max-width: 720px;
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .stack {
    display: grid;
    gap: 8px;
    max-width: 480px;
  }
  .stack > .app-control {
    justify-self: start;
  }
  label,
  legend {
    font: var(--weight-strong) 11px var(--font-mono);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font: 13px var(--font-sans);
  }
  .check input {
    accent-color: var(--declare);
    margin: 0;
  }
  fieldset {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .scroll {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-sans);
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-soft);
    text-align: left;
    vertical-align: middle;
    white-space: nowrap;
  }
  th {
    font: 700 11px var(--font-mono);
    color: var(--muted);
  }
  .bad {
    color: var(--danger);
  }
  [role='status']:empty {
    display: none;
  }
  @media (max-width: 767px) {
    header,
    .body {
      padding-left: 16px;
      padding-right: 16px;
    }
  }
</style>
