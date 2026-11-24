<!-- The answer to every email-proof request: identical whatever the address, with the message's code to paste. -->
<script>
  let { address = '', onverify, again, againLabel = 'Use a different address' } = $props();
  let code = $state('');
  let busy = $state(false);
  function submit(event) {
    event.preventDefault();
    if (!code.trim()) return document.getElementById('proof').focus();
    busy = true;
    onverify(code.trim());
  }
</script>

<p class="done" role="status" data-testid="check-email">
  <span class="ok">✓</span> Check your email.{#if address}{' '}If mail reaches <b>{address}</b>, it holds a link.{/if}
  Open the link in this browser within fifteen minutes, or paste the code from the message here.
</p>
<form class="fields" novalidate onsubmit={submit}>
  <label for="proof">Code from the email</label>
  <input id="proof" class="app-input mono" name="proof" autocomplete="one-time-code" autocapitalize="none" spellcheck="false" readonly={busy} bind:value={code} />
  <button class="app-control primary wide" type="submit" disabled={busy} aria-busy={busy ? 'true' : undefined} data-testid="verify">{busy ? 'Checking…' : 'Verify'}</button>
</form>
{#if again}<nav class="links"><a href={again}>{againLabel}</a></nav>{/if}
