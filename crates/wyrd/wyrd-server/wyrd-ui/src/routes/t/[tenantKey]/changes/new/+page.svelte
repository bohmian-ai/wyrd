<script lang="ts">
  import '$lib/features/changes/changes.css';
  import DraftForm from '$lib/features/changes/DraftForm.svelte';
  let { data, form } = $props();
</script>
<svelte:head><title>New Change Request · Wyrd</title></svelte:head>
<div class="changes"><DraftForm verifiers={data.verifiers} initial={data.draft} csrf={data.session.csrf} requestKey={data.requestKey} base={`/t/${data.tenant.key}/changes`} result={form} /></div>
