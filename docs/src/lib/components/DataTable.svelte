<script lang="ts">
  // The one reusable doc table. A plain markdown `| … |` table already gets the
  // shared frame automatically — reach for this only when a table needs semantic
  // column roles (violet identifier, lime type, a status badge) or is generated
  // from data. A column `role` styles that whole column; an individual cell may
  // override with an object form for a badge or inline HTML. Styling is the
  // shared `.mk table.ref` recipe in styles/arcade.css, so this maps data →
  // semantic markup and owns no colors of its own.
  type Role = 'name' | 'type';
  type Tone = 'warn' | 'ok' | 'danger';
  type Column = { header: string; role?: Role };
  type Cell = string | { text: string; badge?: Tone; html?: boolean };

  let { columns, rows }: { columns: Column[]; rows: Cell[][] } = $props();

  const roleClass = (role: Role | undefined) =>
    role === 'name' ? 'nm' : role === 'type' ? 'ty' : undefined;
</script>

<table class="ref">
  <thead>
    <tr>
      {#each columns as col (col.header)}<th>{col.header}</th>{/each}
    </tr>
  </thead>
  <tbody>
    {#each rows as row, i (i)}
      <tr>
        {#each row as cell, c (c)}
          {@const col = columns[c]}
          <td class={roleClass(col?.role)}>
            {#if typeof cell === 'object'}
              {#if cell.badge}<span class="st {cell.badge}">{cell.text}</span>
              {:else if cell.html}<!-- repo-authored / schema-derived text, never user input -->{@html cell.text}
              {:else}{cell.text}{/if}
            {:else}{cell}{/if}
          </td>
        {/each}
      </tr>
    {/each}
  </tbody>
</table>
