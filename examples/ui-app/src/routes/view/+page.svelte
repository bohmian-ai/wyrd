<!-- A view document rendered by WyrdView; each panel loads through this app's server. -->
<script lang="ts">
  import { remoteLoader, WyrdView } from 'wyrd-ai-ui/view';
  import { dashboard } from '../../panels';

  /** The document as last arranged; edits live only for this page visit. */
  let doc = $state.raw<unknown>(dashboard);
</script>

<main>
  <WyrdView {doc} load={remoteLoader('/view/load')} onlayout={(next) => (doc = next)} />
  <a href="/">The same panels as components</a>
</main>

<style>
  main {
    display: grid;
    gap: 16px;
    max-width: 960px;
    margin: 0 auto;
    padding: 16px;
  }
</style>
