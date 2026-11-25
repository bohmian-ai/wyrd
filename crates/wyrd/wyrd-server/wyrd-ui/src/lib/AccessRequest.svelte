<!--
  Guidance for a person who cannot reach something: ask an administrator, with
  a request to copy into their own channel. The text holds only what the person
  already has: their account, the tenant, the action and the link they opened.
  Nothing is looked up, sent or stored, so an unknown target and an inaccessible
  one read the same.
-->
<script lang="ts">
  import { page } from '$app/state';
  import { accessRequest } from '$lib/access';
  /** The action the person wanted, in plain words. */
  let { action }: { action: string } = $props();
  let copied = $state(false);
  async function copy() {
    await navigator.clipboard.writeText(
      accessRequest({ identity: page.data.session.subject.name, tenant: page.data.tenant.key, action, target: page.url.href })
    );
    copied = true;
  }
</script>

<div class="request">
  <p>
    Ask your Wyrd administrator for access. Copy the request and send it in your own channel with your reason: Wyrd sends nothing and keeps no record of it.
  </p>
  <button class="app-control" type="button" onclick={copy}>{copied ? 'Copied' : 'Copy access request'}</button>
</div>

<style>
  .request {
    display: grid;
    gap: 8px;
    justify-items: start;
    max-width: 560px;
    margin-top: 12px;
  }
  p {
    margin: 0;
    font: 12px/18px var(--font-sans);
    color: var(--muted);
  }
</style>
