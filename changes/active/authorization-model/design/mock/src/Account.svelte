<!-- #/t/<slug>/settings/account: the new "Recovery email" section of the existing Account page. -->
<script>
  import Problem from './Problem.svelte';
  import CheckEmail from './CheckEmail.svelte';
  import { P, emailError } from './data.js';
  let { tenant, query } = $props();
  // App re-creates the page on every hash change, so reading the props once is the whole lifetime.
  const q = (key) => query.get(key);
  const preview = q('preview');
  let issue = $state.raw({ recent: P.recent, unavailable: P.contactOff, throttled: P.emailThrottled }[preview] ?? null);
  let email = $state(preview && preview !== 'unavailable' ? 'sam.recovery@example.test' : '');
  let checked = $state(false);
  let sent = $state(preview === 'sent');
  const error = $derived(checked ? emailError(email) : '');

  // Simulates POST /auth/browser/email/request {tenant, email}: needs a password or provider sign-in from the last
  // five minutes; answers 202 check_email and sets the flow cookie.
  function request(event) {
    event.preventDefault();
    checked = true;
    if (error) return document.getElementById('recovery-email').focus();
    issue = null;
    sent = true;
  }
  // Simulates POST /auth/browser/email/verify {tenant, proof}: 204.
  const verify = () => setTimeout(() => { location.hash = `#/t/${tenant}/contact/verify?preview=proven`; }, 600);
</script>

<main class="page">
  <p class="placeholder">Existing Account page for <code>{tenant}</code>: profile, password and API keys stay as they are. Only the section below is new.</p>
  <section aria-labelledby="recovery">
    <h2 id="recovery">Recovery email</h2>
    <p>Wyrd sends password reset codes for this account to your verified recovery email, and lists this workspace when you ask which workspaces the address can sign in to. Verifying a new address replaces the old one, which is told of the change.</p>
    {#if issue}<Problem {issue} />{/if}
    {#if preview === 'recent'}
      <a class="app-control primary" href="#/t/{tenant}/login">Sign in again</a>
    {:else if preview === 'unavailable'}
      <!-- nothing to submit -->
    {:else if sent}
      <div class="stack"><CheckEmail address={email.trim()} onverify={verify} again="#/t/{tenant}/settings/account" /></div>
    {:else}
      <form class="stack" novalidate onsubmit={request}>
        <label for="recovery-email">Email</label>
        <input id="recovery-email" class="app-input" type="email" name="email" autocomplete="email" bind:value={email}
          aria-invalid={error ? 'true' : undefined} aria-describedby={error ? 'recovery-error' : undefined} />
        {#if error}<p class="field-error" id="recovery-error">⊘ {error}</p>{/if}
        <p>Needs a sign-in from the last five minutes. Your sign-in, password and access do not change.</p>
        <button class="app-control primary" type="submit" data-testid="contact-start">Send verification link</button>
      </form>
    {/if}
  </section>
</main>

<style>
  .page {
    display: grid;
    gap: 20px;
    align-content: start;
    max-width: 720px;
    padding: 24px;
  }
  .placeholder {
    padding: 12px;
    border: 1px dashed var(--border);
    border-radius: var(--r);
  }
  section {
    display: grid;
    gap: 12px;
    min-width: 0;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  h2 {
    margin: 0;
    font: var(--weight-strong) 13px var(--font-sans);
  }
  p {
    font: 12px/18px var(--font-sans);
    color: var(--muted);
  }
  .stack {
    display: grid;
    gap: 8px;
    max-width: 480px;
  }
  .stack :global(form.fields) {
    display: grid;
    gap: 8px;
  }
  .stack :global(.wide) {
    justify-self: start;
  }
  .stack :global(.links) {
    font: 12px var(--font-sans);
  }
  .stack :global(.links a) {
    color: var(--muted);
  }
  label,
  .stack :global(label) {
    font: var(--weight-strong) 11px var(--font-mono);
  }
  a.app-control,
  button.app-control {
    justify-self: start;
  }
  @media (max-width: 767px) {
    .page {
      padding: 16px;
    }
  }
</style>
