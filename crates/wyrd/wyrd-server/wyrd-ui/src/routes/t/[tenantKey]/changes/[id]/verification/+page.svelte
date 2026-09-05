<script lang="ts">
  import '$lib/features/changes/changes.css';
  import ChangeHeader from '$lib/features/changes/ChangeHeader.svelte';
  import Verification from '$lib/features/changes/Verification.svelte';
  let { data, form } = $props();
  let base = $derived(`/t/${data.tenant.key}/changes/${data.change.id}`);
</script>
<div class="changes"><ChangeHeader view={data} {base} active="Verification" /><Verification view={data} csrf={data.session.csrf} {base} result={form} /></div>
