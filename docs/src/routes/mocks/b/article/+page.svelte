<script lang="ts">
  import Frame from '$lib/mocks/Frame.svelte';
  import Sidebar from '$lib/mocks/Sidebar.svelte';
  import Toc from '$lib/mocks/Toc.svelte';
  import ArticleBody from '$lib/mocks/ArticleBody.svelte';
</script>

<Frame dir="b" product="wyrd">
  <div class="shell">
    <aside class="side"><Sidebar active="register-cards" /></aside>
    <main class="main"><ArticleBody /></main>
    <aside><Toc /></aside>
  </div>
</Frame>
