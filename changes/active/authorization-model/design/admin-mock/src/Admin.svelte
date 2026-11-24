<!-- Tenant Admin: a visible section list beside one work region. -->
<script>
  import Icon from './Icon.svelte';
  import { I } from './icons.js';
  import { USERS, SERVICES, ROLES, SECRETS } from './data.js';
  import People from './People.svelte';
  import Services from './Services.svelte';
  import Roles from './Roles.svelte';
  import SignIn from './SignIn.svelte';
  import Secrets from './Secrets.svelte';
  import Integrations from './Integrations.svelte';
  let { section, query } = $props();
  const s = '#/t/gtest/settings';
  const groups = [
    ['Access', [
      ['people', 'People', I.people, USERS.length],
      ['service-accounts', 'Service accounts', I.service, SERVICES.length],
      ['roles', 'Roles', I.roles, ROLES.length],
    ]],
    ['Security', [
      ['sign-in', 'Sign-in', I.signin],
      ['secrets', 'Secrets', I.secrets, SECRETS.filter((x) => x.state === 'active').length],
    ]],
    ['Workspace', [['integrations', 'Integrations', I.integrations]]],
  ];
</script>

<div class="admin">
  <nav class="sections" aria-label="Settings">
    {#each groups as [title, items] (title)}
      <div class="group">
        <h2>{title}</h2>
        {#each items as [key, label, icon, count, attn] (key)}
          <a href="{s}/{key}" aria-current={section === key ? 'page' : undefined}>
            <Icon d={icon} />{label}
            {#if attn}<span class="attn" aria-label="needs attention">{attn}</span>{:else if count != null}<span class="count">{count}</span>{/if}
          </a>
        {/each}
      </div>
    {/each}
  </nav>
  <main class="work">
    {#if section === 'people'}<People {query} />
    {:else if section === 'service-accounts'}<Services />
    {:else if section === 'roles'}<Roles {query} />
    {:else if section === 'sign-in'}<SignIn {query} />
    {:else if section === 'secrets'}<Secrets {query} />
    {:else}<Integrations />{/if}
  </main>
</div>
