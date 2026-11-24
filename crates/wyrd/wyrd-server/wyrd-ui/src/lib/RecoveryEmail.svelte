<!--
  The Account page's Recovery email section. The browser asks Rust to mail a
  proof to the new address, because Rust scopes that flow's cookie to
  `/auth/browser/email`; the message's link (or a pasted code) opens the
  tenant's contact verify page. No current address is shown.
-->
<script lang="ts">
  import CheckEmail from '$lib/CheckEmail.svelte';
  import { guidance } from '$lib/Entry.svelte';
  import { PROBLEMS, auth, emailError, openProof, refusal } from '$lib/onboarding';
  import type { WyrdProblem } from '$lib/server/problem';
  let { tenant }: { tenant: string } = $props();

  let issue: WyrdProblem | null = $state.raw(null);
  let email = $state('');
  let checked = $state(false);
  let sent = $state(false);
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();
  const error = $derived(checked ? emailError(email) : '');
  const verifyPage = $derived(`/t/${encodeURIComponent(tenant)}/contact/verify`);

  async function request(event: SubmitEvent) {
    event.preventDefault();
    checked = true;
    if (error) return input?.focus();
    busy = true;
    const answer = await auth('/auth/browser/email/request', { tenant, email: email.trim() });
    busy = false;
    sent = answer.status === 202;
    issue = sent
      ? null
      : refusal(answer, {
          WYRD_AUTH_403_RECENT_LOGIN_REQUIRED: PROBLEMS.recent,
          WYRD_AUTH_403_METHOD_UNAVAILABLE: PROBLEMS.contactOff,
          WYRD_AUTH_401_INVALID_TOKEN: PROBLEMS.signedOut
        });
  }
</script>

<section aria-labelledby="recovery">
  <h2 id="recovery">Recovery email</h2>
  <p>Wyrd sends password reset codes for this account to your verified recovery email, and lists this workspace when you ask which workspaces the address can sign in to. Verifying a new address replaces the old one, which is told of the change.</p>
  {#if issue}
    <div class="app-problem" role="alert" data-state={issue.status >= 500 ? 'error' : 'unauthorized'}>
      <strong>⊘ {issue.title}</strong>
      <p>{guidance(issue)}</p>
      <code>{issue.code}</code>
    </div>
  {/if}
  {#if issue === PROBLEMS.recent}
    <a class="app-control primary" href="/t/{encodeURIComponent(tenant)}/login">Sign in again</a>
  {:else if sent}
    <div class="stack"><CheckEmail address={email.trim()} onverify={async (code) => openProof(verifyPage, code)} again="/t/{encodeURIComponent(tenant)}/settings/account" /></div>
  {:else if issue !== PROBLEMS.contactOff}
    <form class="stack" novalidate onsubmit={request}>
      <label for="recovery-email">Email</label>
      <input id="recovery-email" class="app-input" type="email" name="email" autocomplete="email" readonly={busy} bind:this={input} bind:value={email}
        aria-invalid={error ? 'true' : undefined} aria-describedby={error ? 'recovery-error' : undefined} />
      {#if error}<p class="field-error" id="recovery-error">⊘ {error}</p>{/if}
      <p>Needs a sign-in from the last five minutes. Your sign-in, password and access do not change.</p>
      <button class="app-control primary" type="submit" disabled={busy}>Send verification link</button>
    </form>
  {/if}
</section>

<style>
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
    margin: 0;
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
</style>
