<script>
  // One fact with glyph and word; hue is never the only channel.
  let { f, name } = $props();
</script>

<span class="st {f.cls}" {...{ [`data-${name}`]: f.key }}><span aria-hidden="true">{f.glyph}</span>{f.word}</span>
