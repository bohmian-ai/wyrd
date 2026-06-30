<script lang="ts">
  import { base } from '$app/paths';
  import Frame from '$lib/mocks/Frame.svelte';
  import Plane from '$lib/mocks/Plane.svelte';
</script>

<Frame dir="a" product="wyrd">
  <section class="nf">
    <Plane />
    <div class="nf-in">
      <div class="big">404</div>
      <div class="go">GAME OVER — page not found</div>
      <p class="lead" style="margin:0 auto 8px">
        That route was never registered. No Card lives here.
      </p>
      <div class="coin">INSERT COIN TO CONTINUE</div>
      <div class="hero-cta" style="justify-content:center">
        <a class="btn btn-lime" href={`${base}/mocks/a/landing/`}>▶ Back to start</a>
        <a class="btn btn-ghost" href={`${base}/mocks/a/reference/`}>Search the docs</a>
      </div>
    </div>
  </section>
</Frame>
