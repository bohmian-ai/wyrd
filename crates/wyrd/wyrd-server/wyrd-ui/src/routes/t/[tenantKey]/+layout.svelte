<!--
  The signed-in tenant shell: the tenant's destinations, Settings, and the
  name block, which opens Your account. Destinations this release does not
  deliver, and Ask, stay visible but disabled.
-->
<script lang="ts">
  import type { LayoutProps } from './$types';
  import { ICONS } from '$lib/icons';
  import Shell, { type Destination } from '$lib/Shell.svelte';
  let { data, children }: LayoutProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const groups = $derived<readonly (readonly Destination[])[]>([
    [
      { label: 'Home', href: base, icon: ICONS.home },
      { label: 'Cards', href: `${base}/cards`, icon: ICONS.cards },
      { label: 'Repositories', href: `${base}/repositories`, icon: ICONS.repositories }
    ],
    [
      { label: 'Observe', href: `${base}/observe`, icon: ICONS.observe },
      { label: 'Verify', href: `${base}/verify`, icon: ICONS.verify },
      { label: 'Query', href: `${base}/query`, icon: ICONS.query }
    ]
  ]);
  const me = $derived({
    name: data.me?.name ?? data.session.subject.name,
    detail: data.me?.email ?? data.tenant.name,
    href: `${base}/settings/account`
  });
</script>

<Shell home={base} {groups} settings={`${base}/settings`} find {me}>{@render children()}</Shell>
