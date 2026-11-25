<!--
  Name the workspace this browser's live proof creates. GET /auth/signup/proof
  says how the proof was made: a mailbox proof chooses the first
  administrator's username and password, a provider proof signs in with
  that provider and sets none. A configured challenge renders Cloudflare's
  Turnstile widget, whose single-use token completion sends.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import Entry from '$lib/Entry.svelte';
  import CheckEmail from '$lib/CheckEmail.svelte';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import { PROBLEMS, PROVIDERS, auth, emailError, passwordError, refusal, slugError, slugify, turnstile, verifySignup } from '$lib/onboarding';
  import type { WyrdProblem } from '$lib/server/problem';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();

  let method: string | null = $state(null);
  // svelte-ignore state_referenced_locally -- the load's problem is the page's first state
  let issue: WyrdProblem | null = $state.raw(data.problem);
  let name = $state('');
  let slug = $state('');
  let suggested = $state(true);
  let checked = $state(false);
  let phase: 'idle' | 'creating' | 'created' = $state('idle');
  let username = $state('');
  let password = $state('');
  let contactEmail = $state('');
  let contactSent = $state(false);
  let contactProven = $state(false);
  let challengeToken = $state('');
  let widget: string | undefined;

  const local = $derived(method === 'email');
  const contactProblem = $derived(data.mail ? PROBLEMS.contact : PROBLEMS.contactNoMail);
  const nameError = $derived(checked && !name.trim() ? 'Enter a workspace name.' : '');
  const slugProblem = $derived(checked ? slugError(slug) : '');
  const takenSlug = $derived(issue === PROBLEMS.taken);
  // A refusal that ends this attempt leaves nothing to submit.
  const ended = $derived(issue === PROBLEMS.disabled || issue === PROBLEMS.limit || issue === PROBLEMS.expired || issue === PROBLEMS.contactNoMail);
  // Past the limit or with signup off, an existing workspace is the way on; otherwise a fresh proof.
  const signIn = $derived(issue === PROBLEMS.disabled || issue === PROBLEMS.limit);
  const userError = $derived(local && checked && !username.trim() ? 'Enter a username.' : '');
  const pwError = $derived(local && checked ? passwordError(password) : '');
  const contactError = $derived(contactSent || !contactEmail ? '' : emailError(contactEmail));
  const busy = $derived(phase !== 'idle');

  onMount(async () => {
    const answer = await auth('/auth/signup/proof');
    if (answer.status === 200 && typeof answer.body?.method === 'string') method = answer.body.method;
    else issue = answer.status === 401 ? PROBLEMS.expired : refusal(answer);
  });

  /**
   * Render the Turnstile widget for `sitekey` into `element`, unless the
   * element is gone by the time the script loads; its token is single-use.
   */
  function challengeWidget(element: HTMLElement, sitekey: string) {
    let gone = false;
    turnstile()
      .then((api) => {
        if (gone) return;
        const clear = () => (challengeToken = '');
        widget = api.render(element, { sitekey, action: 'signup', callback: (token: string) => (challengeToken = token), 'expired-callback': clear, 'error-callback': clear });
      })
      // An unloadable widget leaves no token; Rust refuses the completion as a failed challenge.
      .catch(() => {});
    return {
      destroy() {
        gone = true;
        const rendered = widget;
        widget = undefined;
        challengeToken = '';
        if (rendered !== undefined) void turnstile().then((api) => api.remove(rendered));
      }
    };
  }

  function rename(typed: string) {
    if (suggested) slug = slugify(typed);
  }
  function reslug() {
    suggested = false;
    checked = true;
    if (takenSlug) issue = null;
  }

  /** The problem a refused completion shows, in the mock's words. */
  function completionRefusal(answer: Awaited<ReturnType<typeof auth>>): WyrdProblem {
    const details = (answer.body?.details ?? {}) as { reason?: string; field?: string; tenant?: string };
    if (answer.status === 409) {
      if (details.reason === 'contact_required') return contactProblem;
      if (details.reason === 'workspace_created') return PROBLEMS.created;
      // An interrupted earlier attempt already claimed its slug: finish that one.
      if (details.field === 'slug' && details.tenant) slug = details.tenant;
      return PROBLEMS.taken;
    }
    return refusal(answer, {
      WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT: PROBLEMS.limit,
      WYRD_AUTH_403_SIGNUP_CHALLENGE: PROBLEMS.challenge,
      WYRD_AUTH_403_METHOD_UNAVAILABLE: PROBLEMS.disabled,
      WYRD_AUTH_401_INVALID_TOKEN: PROBLEMS.disabled,
      WYRD_AUTH_400_INVALID_STATE: PROBLEMS.expired
    });
  }

  async function create(event: SubmitEvent) {
    event.preventDefault();
    checked = true;
    const first = !name.trim() ? 'display-name' : slugError(slug) ? 'slug' : local && !username.trim() ? 'username' : local && passwordError(password) ? 'password' : '';
    if (first) return document.getElementById(first)?.focus();
    issue = null;
    phase = 'creating';
    const answer = await auth('/auth/signup/complete', {
      slug,
      display_name: name.trim(),
      ...(local ? { local_credentials: { username: username.trim(), password } } : {}),
      ...(challengeToken ? { challenge_token: challengeToken } : {})
    });
    if (answer.status !== 200) {
      issue = completionRefusal(answer);
      // Siteverify spent the token; the widget issues a fresh one.
      if (widget !== undefined) {
        challengeToken = '';
        (await turnstile()).reset(widget);
      }
      phase = 'idle';
      return;
    }
    phase = 'created';
    // A repeated completion answers without a session; the workspace then asks its creator to sign in.
    location.assign(`/t/${encodeURIComponent(String(answer.body?.tenant ?? slug))}`);
  }

  // The provider attempt's completion cookie binds this mailbox to it.
  async function sendContact(event: SubmitEvent) {
    event.preventDefault();
    if (emailError(contactEmail)) return document.getElementById('contact-email')?.focus();
    const answer = await auth('/auth/signup', { method: 'email', email: contactEmail.trim() });
    if (answer.status === 202) contactSent = true;
    else issue = refusal(answer);
  }
  async function contactVerified(proof: string) {
    const refused = await verifySignup(proof);
    if (refused) issue = refused;
    else {
      contactProven = true;
      issue = null;
    }
  }
</script>

<svelte:head><title>Name your workspace · Wyrd</title></svelte:head>
<Entry heading="Name your workspace" {issue}>
  {#if ended}
    <a class="app-control primary wide" href={signIn ? '/' : '/signup'}>{signIn ? 'Sign in to an existing workspace' : 'Start again'}</a>
  {:else if !method}
    <p class="center" role="status" aria-busy="true">Checking this browser’s sign-in…</p>
  {:else}
    {#if issue === PROBLEMS.contact}
      {#if contactSent}
        <CheckEmail address={contactEmail.trim()} onverify={contactVerified} />
      {:else}
        <form class="fields" novalidate onsubmit={sendContact}>
          <label for="contact-email">Email to confirm</label>
          <input id="contact-email" class="app-input" type="email" autocomplete="email" bind:value={contactEmail}
            aria-invalid={contactError ? 'true' : undefined} aria-describedby={contactError ? 'contact-error' : undefined} />
          {#if contactError}<p class="field-error" id="contact-error">⊘ {contactError}</p>{/if}
          <button class="app-control wide" type="submit">Send a link</button>
        </form>
      {/if}
    {/if}
    {#if contactProven}<p class="center done" role="status"><span class="ok">✓</span> Email confirmed. Create the workspace again.</p>{/if}
    <p class="center" data-testid="identity">
      {#if local}Email confirmed in this browser. Choose the username and password you will sign in with.{:else}Signed in with {PROVIDERS[method] ?? method}.{/if}
    </p>
    <form class="fields" novalidate onsubmit={create}>
      <label for="display-name">Workspace name</label>
      <input id="display-name" class="app-input" name="display_name" required maxlength="128" autocomplete="organization" readonly={busy}
        aria-invalid={nameError ? 'true' : undefined} aria-describedby={nameError ? 'name-error' : undefined} bind:value={name} oninput={(event) => rename(event.currentTarget.value)} />
      {#if nameError}<p class="field-error" id="name-error">⊘ {nameError}</p>{/if}

      <label for="slug">URL slug</label>
      <input id="slug" class="app-input mono" name="slug" required minlength="3" maxlength="63" autocomplete="off" autocapitalize="none" spellcheck="false" readonly={busy}
        aria-invalid={slugProblem || takenSlug ? 'true' : undefined}
        aria-describedby="slug-url slug-hint{slugProblem ? ' slug-error' : ''}{takenSlug ? ' problem' : ''}" bind:value={slug} oninput={reslug} />
      <p id="slug-url">Workspace URL <code>/t/{slug || '<slug>'}</code></p>
      {#if slugProblem}
        <p class="field-error" id="slug-error">⊘ {slugProblem}</p>
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
        <PasswordInput id="password" name="password" required autocomplete="new-password" readonly={busy}
          aria-invalid={pwError ? 'true' : undefined} aria-describedby="password-hint{pwError ? ' password-error' : ''}" bind:value={password} />
        {#if pwError}<p class="field-error" id="password-error">⊘ {pwError}</p>{/if}
        <p id="password-hint">15 to 128 characters. You sign in to this workspace with this username and password.</p>
      {/if}

      {#if data.challenge}<div class="turnstile" data-testid="turnstile" use:challengeWidget={data.challenge}></div>{/if}

      <button class="app-control primary wide" type="submit" disabled={busy} aria-busy={phase === 'creating' ? 'true' : undefined}>
        {phase === 'creating' ? 'Creating workspace…' : 'Create workspace'}
      </button>
    </form>
    <div role="status" aria-live="polite">
      {#if phase === 'created'}<p class="center done"><span class="ok">✓</span> Workspace created. Opening <code>/t/{slug}</code>…</p>{/if}
    </div>
    <nav class="links" aria-label="Existing workspace"><a href="/">Already have a workspace? Sign in</a></nav>
  {/if}
</Entry>

<style>
  .center {
    text-align: center;
  }
  [role='status']:empty {
    display: none;
  }
  .turnstile {
    margin: 6px 0;
  }
  button[type='submit'] {
    margin-top: 6px;
  }
</style>
