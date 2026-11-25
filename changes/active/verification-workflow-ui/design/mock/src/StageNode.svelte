<script>
  // A stage heading: part of the drawing so it pans and zooms with the columns it names. Not interactive.
  let { data } = $props();
</script>

<div class="stage h" data-stage={data.text}>{data.text}</div>
