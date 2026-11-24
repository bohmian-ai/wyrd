<!--
  The page a recovery-contact link opens. It needs this tenant's session in
  this browser, so the hook lets a signed-out visitor see why instead of
  redirecting away from the link.
-->
<script lang="ts">
  import { page } from '$app/state';
  import Proof from '$lib/Proof.svelte';
  import { PROBLEMS, PROOF_REFUSALS, auth, refusal } from '$lib/onboarding';

  const tenant = page.params.tenantKey ?? '';
  const account = `/t/${encodeURIComponent(tenant)}/settings/account`;
  let signedOut = $state(false);

  async function verify(proof: string) {
    const answer = await auth('/auth/browser/email/verify', { tenant, proof });
    if (answer.status === 204) return null;
    const issue = refusal(answer, { ...PROOF_REFUSALS, WYRD_AUTH_401_INVALID_TOKEN: PROBLEMS.signedOut });
    signedOut = issue === PROBLEMS.signedOut;
    return issue;
  }
</script>

<svelte:head><title>Verify your recovery email · {tenant} · Wyrd</title></svelte:head>
<Proof
  heading="Verify your recovery email"
  doneHeading="Recovery email verified"
  {verify}
  restart={signedOut ? [`/t/${encodeURIComponent(tenant)}/login`, `Sign in to ${tenant}`] : [account, 'Back to account']}
>
  {#snippet done()}
    <p class="done center" role="status"><span class="ok">✓</span> Password reset codes for your {tenant} account now go to this address. A previous recovery email was told of the change.</p>
    <a class="app-control wide" href={account}>Back to account</a>
  {/snippet}
</Proof>

<style>
  .center {
    text-align: center;
  }
</style>
