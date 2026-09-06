<script lang="ts">
  import '$lib/features/changes/changes.css';
  import ChangeHeader from '$lib/features/changes/ChangeHeader.svelte';
  import Review from '$lib/features/changes/Review.svelte';
  let { data, form } = $props();
  let base = $derived(`/t/${data.tenant.key}/changes/${data.change.id}`);
</script>
<div class="changes"><ChangeHeader view={data} {base} active="Conversation" />{#key data.change.id}<Review view={data} csrf={data.session.csrf} actor={data.session.subject.id} {base} result={form} />{/key}</div>
