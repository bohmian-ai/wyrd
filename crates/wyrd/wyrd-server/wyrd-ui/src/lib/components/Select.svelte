<script lang="ts">
  // Filter and time-range control. A native <select> — it is keyboard operable, screen
  // reader labeled and mobile friendly without a line of local menu code. `auto` submits
  // the enclosing form on change so a filter applies immediately; the component never
  // takes a callback, so the destination stays a server-owned form action or GET route.
  type Option = { value: string; label: string };
  let {
    label,
    name,
    options,
    value,
    auto = true
  }: {
    label: string;
    name: string;
    options: Option[];
    value?: string;
    auto?: boolean;
  } = $props();

  const id = $derived(`wy-sel-${name}`);

  function submit(event: Event): void {
    if (!auto) return;
    (event.currentTarget as HTMLSelectElement).form?.requestSubmit();
  }
</script>

<span class="wy-select">
  <label for={id}>{label}</label>
  <span class="field">
    <select {id} {name} value={value ?? options[0]?.value} onchange={submit}>
      {#each options as o (o.value)}
        <option value={o.value}>{o.label}</option>
      {/each}
    </select>
  </span>
</span>

<style>
  .wy-select {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--fm);
    min-width: 0;
  }
  label {
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
  }
  /* The control keeps the workbench geometry — 2px border, 5px radius, 3px hard shadow,
     the same press feedback as Button — while the element underneath stays a real
     <select>. appearance:none drops the platform chrome so the caret is ours; the option
     list itself is still drawn by the OS, which is the trade we make for free keyboard
     operation, a real mobile picker and correct screen-reader semantics. */
  .field {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }
  select {
    appearance: none;
    font-family: var(--fm);
    font-size: 11px;
    font-weight: 700;
    padding: 7px 26px 7px 10px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    background: var(--surface);
    color: var(--text);
    max-width: 100%;
    cursor: pointer;
    transition:
      transform 0.04s,
      box-shadow 0.04s;
  }
  select:hover {
    transform: translate(-1px, -1px);
    box-shadow: 5px 5px 0 0 var(--shadow);
  }
  select:active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0 0 var(--shadow);
  }
  /* the caret is drawn on the wrapper so it never intercepts the click */
  .field::after {
    content: '▾';
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 9px;
    color: var(--muted);
    pointer-events: none;
  }
  /* the default ring is low contrast on the brand and lime fills */
  select:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
</style>
