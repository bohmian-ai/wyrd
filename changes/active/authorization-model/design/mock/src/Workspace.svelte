<!-- #/signup/workspace: after provider proof, name the workspace and choose its URL slug. -->
<script>
  import Entry from './Entry.svelte';
  import CheckEmail from './CheckEmail.svelte';
  import { PROVIDERS, IDENTITIES, PROBLEMS, P, TAKEN, slugify, slugError, passwordError, emailError } from './data.js';
  let { query } = $props();

  // Each preview state starts as the server would leave the page after that answer.
  // App re-creates this page on every hash change, so reading the query once is the whole lifetime.
  const param = (key) => query.get(key);
  const preview = param('preview');
  const identity = IDENTITIES[param('identity')] ?? IDENTITIES.google;
  const challenge = param('challenge') === 'on';
  // An email proof needs local_credentials; a provider proof refuses them.
  const local = identity.provider === 'email';
  // GET /auth/browser/methods: local_signup, i.e. a relay exists for the contact email branch.
  const mail = param('local') !== 'off';
  const unverified = identity.provider !== 'email' && !identity.email;
  const contactProblem = mail ? PROBLEMS.contact : P.contactNoMail;
  const sample = { invalid: ['Acme Research', '-Acme_Research'], taken: ['Acme', 'acme'], challenge: ['Acme Research', 'acme-research'] }[preview]
    ?? (preview ? ['Acme Research', 'acme-research'] : ['', '']);

  let name = $state(sample[0]);
  let slug = $state(sample[1]);
  let derived = $state(!preview);
  let checked = $state(preview === 'invalid');
  let human = $state(false);
  let phase = $state(preview === 'creating' ? 'creating' : preview === 'created' ? 'created' : 'idle');
  // Raw: refusals are compared by identity below.
  let issue = $state.raw(preview === 'contact' ? contactProblem : PROBLEMS[preview] ?? null);
  let username = $state('');
  let password = $state('');
  let contactEmail = $state('');
  let contactSent = $state(false);
  let contactProven = $state(false);

  const nameError = $derived(checked && !name.trim() ? 'Enter a workspace name.' : '');
  const slugProblem = $derived(checked ? slugError(slug) : '');
  const takenSlug = $derived(issue === PROBLEMS.taken);
  // A refusal that ends this attempt leaves nothing to submit.
  const ended = $derived(issue === PROBLEMS.disabled || issue === PROBLEMS.expired || issue === P.contactNoMail);
  const userError = $derived(local && checked && !username.trim() ? 'Enter a username.' : '');
  const pwError = $derived(local && checked ? passwordError(password) : '');
  const contactError = $derived(contactSent || !contactEmail ? '' : emailError(contactEmail));
  const busy = $derived(phase !== 'idle');

  function rename() {
    if (derived) slug = slugify(name);
  }
  function reslug() {
    derived = false;
    checked = true;
    if (takenSlug) issue = null;
  }

  // Simulates POST /auth/signup/complete with the sample data.
  function create(event) {
    event.preventDefault();
    checked = true;
    const first = !name.trim() ? 'display-name' : slugError(slug) ? 'slug' : local && !username.trim() ? 'username' : local && passwordError(password) ? 'password' : '';
    if (first) {
      document.getElementById(first).focus();
      return;
    }
    issue = null;
    phase = 'creating';
    setTimeout(() => {
      const refusal = challenge && !human ? PROBLEMS.challenge : TAKEN.includes(slug) ? PROBLEMS.taken : unverified && !contactProven ? contactProblem : null;
      if (refusal) {
        issue = refusal;
        phase = 'idle';
        return;
      }
      phase = 'created';
      setTimeout(() => { location.hash = `#/t/${slug}`; }, 1200);
    }, 900);
  }

  // Simulates POST /auth/signup {method:"email", email} with the provider attempt's completion cookie, then
  // POST /auth/signup/verify: the mailbox is bound to this provider attempt.
  function sendContact(event) {
    event.preventDefault();
    if (emailError(contactEmail)) return document.getElementById('contact-email').focus();
    contactSent = true;
  }
  function contactVerified() {
    setTimeout(() => { contactProven = true; issue = null; }, 600);
  }
</script>

<Entry heading="Name your workspace" {issue}>
  {#if ended}
    <a class="app-control primary wide" href={issue === PROBLEMS.disabled ? '#/' : '#/signup'}>{issue === PROBLEMS.disabled ? 'Sign in to an existing workspace' : 'Start again'}</a>
  {:else}
    {#if issue === PROBLEMS.contact}
      {#if contactSent}
        <CheckEmail address={contactEmail.trim()} onverify={contactVerified} />
      {:else}
        <form class="fields" novalidate onsubmit={sendContact} data-testid="contact-branch">
          <label for="contact-email">Email to confirm</label>
          <input id="contact-email" class="app-input" type="email" autocomplete="email" bind:value={contactEmail}
            aria-invalid={contactError ? 'true' : undefined} aria-describedby={contactError ? 'contact-error' : undefined} />
          {#if contactError}<p class="field-error" id="contact-error">⊘ {contactError}</p>{/if}
          <button class="app-control wide" type="submit">Send a link</button>
        </form>
      {/if}
    {/if}
    {#if contactProven}<p class="identity done" role="status" data-testid="contact-proven"><span class="ok">✓</span> Email confirmed. Create the workspace again.</p>{/if}
    <p class="identity" data-testid="identity">
      {#if local}Email confirmed in this browser. Choose the username and password you will sign in with.{:else}Signed in with {PROVIDERS[identity.provider]}{#if identity.email}{' '}as <b>{identity.email}</b>{/if}.{/if}
    </p>
    <form class="fields" novalidate onsubmit={create}>
      <label for="display-name">Workspace name</label>
      <input id="display-name" class="app-input" name="display_name" required maxlength="128" autocomplete="organization" readonly={busy}
        aria-invalid={nameError ? 'true' : undefined} aria-describedby={nameError ? 'name-error' : undefined} bind:value={name} oninput={rename} />
      {#if nameError}<p class="field-error" id="name-error">⊘ {nameError}</p>{/if}

      <label for="slug">URL slug</label>
      <input id="slug" class="app-input mono" name="slug" required minlength="3" maxlength="63" autocomplete="off" autocapitalize="none" spellcheck="false" readonly={busy}
        aria-invalid={slugProblem || takenSlug ? 'true' : undefined}
        aria-describedby="slug-url slug-hint{slugProblem ? ' slug-error' : ''}{takenSlug ? ' problem' : ''}" bind:value={slug} oninput={reslug} />
      <p id="slug-url" class="url">Workspace URL <code data-testid="url">/t/{slug || '<slug>'}</code></p>
      {#if slugProblem}
        <p class="field-error" id="slug-error" data-testid="slug-error">⊘ {slugProblem}</p>
      {:else if takenSlug}
        <p class="field-error">⊘ <code>/t/{slug}</code> belongs to another workspace.</p>
      {/if}
      <p id="slug-hint">3–63 characters: lowercase letters, digits and hyphens, starting and ending with a letter or digit.</p>

      {#if local}
        <label for="username">Your username</label>
        <input id="username" class="app-input" name="username" required autocomplete="username" autocapitalize="none" spellcheck="false" readonly={busy}
          aria-invalid={userError ? 'true' : undefined} aria-describedby={userError ? 'username-error' : undefined} bind:value={username} />
        {#if userError}<p class="field-error" id="username-error">⊘ {userError}</p>{/if}
        <label for="password">Your password</label>
        <input id="password" class="app-input" type="password" name="password" required autocomplete="new-password" readonly={busy}
          aria-invalid={pwError ? 'true' : undefined} aria-describedby="password-hint{pwError ? ' password-error' : ''}" bind:value={password} />
        {#if pwError}<p class="field-error" id="password-error">⊘ {pwError}</p>{/if}
        <p id="password-hint">15 to 128 characters. You sign in to this workspace with this username and password.</p>
      {/if}

      {#if challenge}
        <div class="turnstile" data-testid="turnstile">
          <span class="turnstile-name">Cloudflare Turnstile widget</span>
          <label class="turnstile-check"><input type="checkbox" bind:checked={human} disabled={busy} /> Simulate a passed challenge</label>
          <span class="turnstile-note">placeholder · shown only when the operator configures a challenge</span>
        </div>
      {/if}

      <button class="app-control primary wide" type="submit" disabled={busy} aria-busy={phase === 'creating' ? 'true' : undefined} data-testid="create">
        {phase === 'creating' ? 'Creating workspace…' : 'Create workspace'}
      </button>
    </form>
    <div role="status" aria-live="polite">
      {#if phase === 'created'}<p class="created" data-testid="created"><span class="ok">✓</span> Workspace created. Opening <code>/t/{slug}</code>…</p>{/if}
    </div>
    <nav class="links" aria-label="Existing workspace"><a href="#/">Already have a workspace? Sign in</a></nav>
  {/if}
</Entry>

<style>
  .identity {
    text-align: center;
  }
  p.created {
    color: var(--text);
  }
  .created {
    text-align: center;
  }
  [role='status']:empty {
    display: none;
  }
  .turnstile {
    display: grid;
    gap: 6px;
    margin: 6px 0;
    padding: 14px;
    border: 1px dashed var(--muted);
    border-radius: var(--r);
    background: var(--surface-2);
  }
  .turnstile-name {
    font: var(--weight-strong) 13px var(--font-sans);
  }
  .turnstile label.turnstile-check {
    display: flex;
    align-items: center;
    gap: 8px;
    font: 12px var(--font-sans);
  }
  .turnstile-check input {
    accent-color: var(--declare);
    margin: 0;
  }
  .turnstile-note {
    font: 10px var(--font-mono);
    color: var(--muted);
  }
  button[type='submit'] {
    margin-top: 6px;
  }
</style>
