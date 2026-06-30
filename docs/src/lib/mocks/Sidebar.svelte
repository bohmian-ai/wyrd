<script lang="ts">
  let { active = 'register-cards' }: { active?: string } = $props();

  const groups = [
    {
      label: 'Start here',
      kind: 'client',
      items: [
        ['What is Wyrd', 'what-is-wyrd'],
        ['How it connects', 'how-it-connects'],
        ['Install & quickstart', 'quickstart']
      ]
    },
    {
      label: 'Concepts',
      kind: 'rune',
      items: [
        ['Cards & the envelope', 'cards'],
        ['Specs, Runs, Observations', 'ontology'],
        ['Registry & lineage', 'registry']
      ]
    },
    {
      label: 'Server & auth',
      kind: 'server',
      items: [
        ['Run the server', 'server'],
        ['Authentication', 'auth'],
        ['Register cards', 'register-cards']
      ]
    },
    {
      label: 'Reference',
      kind: 'control',
      items: [
        ['Schema reference', 'schemas'],
        ['Error reference', 'errors'],
        ['For agents', 'agents']
      ]
    }
  ];
</script>

<nav class="side" aria-label="Wyrd documentation">
  {#each groups as g}
    <div class="grp">
      <div class="gh"><span class={`b ${g.kind}`}></span>{g.label}</div>
      {#each g.items as [label, slug]}
        <a href="#top" class={slug === active ? 'on' : ''}>{label}</a>
      {/each}
    </div>
  {/each}
  <div class="grp">
    <div class="gh"><span class="b rune"></span>Fathom</div>
    <a href="#top">Overview <span class="soon">soon</span></a>
  </div>
</nav>
