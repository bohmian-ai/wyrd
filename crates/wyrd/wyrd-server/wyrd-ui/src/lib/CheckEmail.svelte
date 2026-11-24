<!--
  The answer to every email-proof request: identical whatever the address,
  with a field to paste the message's code. `onverify` receives the trimmed
  code and settles when the page has the server's answer.
-->
<script lang="ts">
  let {
    address = '',
    onverify,
    again = '',
    againLabel = 'Use a different address'
  }: { address?: string; onverify: (code: string) => Promise<void>; again?: string; againLabel?: string } = $props();
  let code = $state('');
  let busy = $state(false);
  let input: HTMLInputElement;

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!code.trim()) return input.focus();
    busy = true;
    await onverify(code.trim());
    busy = false;
  }
</script>

<p class="done" role="status" data-testid="check-email">
  <span class="ok">✓</span> Check your email.{#if address}{' '}If mail reaches <b>{address}</b>, it holds a link.{/if}
  Open the link in this browser within fifteen minutes, or paste the code from the message here.
</p>
<form class="fields" novalidate onsubmit={submit}>
  <label for="proof">Code from the email</label>
  <input id="proof" class="app-input mono" name="proof" autocomplete="one-time-code" autocapitalize="none" spellcheck="false" readonly={busy} bind:this={input} bind:value={code} />
  <button class="app-control primary wide" type="submit" disabled={busy} aria-busy={busy ? 'true' : undefined}>{busy ? 'Checking…' : 'Verify'}</button>
</form>
{#if again}<nav class="links"><a href={again}>{againLabel}</a></nav>{/if}
