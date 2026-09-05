<script lang="ts">
  import '$lib/features/changes/changes.css';
  import DraftForm from '$lib/features/changes/DraftForm.svelte';
  import ChangeHeader from '$lib/features/changes/ChangeHeader.svelte';
  import Overview from '$lib/features/changes/Overview.svelte';
  let { data, form } = $props();
</script>
<div class="changes">
  {#if data.editing && data.change.draft}<DraftForm verifiers={data.verifiers} initial={data.change.draft} csrf={data.session.csrf} requestKey={data.requestKey} revision={data.change.revision} base={`/t/${data.tenant.key}/changes`} result={form} />
  {:else}<ChangeHeader view={data} base={`/t/${data.tenant.key}/changes/${data.change.id}`} active="Overview" />{#if data.change.draft}<p><a class="control primary" href="?edit=draft">Resume draft</a></p>{/if}<Overview view={data} csrf={data.session.csrf} result={form} base={`/t/${data.tenant.key}/changes/${data.change.id}`} />{/if}
</div>
