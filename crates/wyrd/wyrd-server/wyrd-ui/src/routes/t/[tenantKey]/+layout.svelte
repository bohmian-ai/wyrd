<script lang="ts">
  import { page } from '$app/state';
  import Shell from '$lib/components/app/Shell.svelte';
  import type { LayoutProps } from './$types';
  let { data, children }: LayoutProps = $props();
</script>

<Shell tenant={data.tenant} session={data.session} pathname={page.url.pathname} {children} />
