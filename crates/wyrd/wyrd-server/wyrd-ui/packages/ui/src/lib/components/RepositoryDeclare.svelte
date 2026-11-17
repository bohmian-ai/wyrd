<!--
  One reachable GitHub repository that has no Repository Card: what is and is
  not true of it, and the file and command that declare it. Nothing here
  registers anything.
-->
<script lang="ts">
  import { ACCESS_WORDS, READINESS_WORDS, type RepositoryDeclareProps } from '../repositories.js';
  import Code from './Code.svelte';
  import CopyButton from './CopyButton.svelte';
  import EmptyState from './EmptyState.svelte';
  import RepositorySheet from './RepositorySheet.svelte';

  let { result }: RepositoryDeclareProps = $props();

  const COMMAND = 'wyrd declare wyrd.toml';
  const access = ACCESS_WORDS.granted;
  const readiness = READINESS_WORDS.undeclared;
</script>

<RepositorySheet>
  {#if result.state === 'populated'}
    {@const data = result.data}
    <div class="xhead">
      <div class="xtitle">
        <div class="xname"><span class="tag">no Repository Card</span><h1 class="mono">{data.fullName}</h1></div>
        <div class="xmeta">github · {data.host} · repository_id {data.repositoryId}</div>
        <p class="desc">GitHub access is granted, but nothing is registered. Access alone never creates a Card, binds a Verifier or starts verification.</p>
      </div>
      <div class="xactions"><a class="btn btn-sm" href="https://{data.host}/{data.fullName}" target="_blank" rel="noopener noreferrer">{data.fullName} ↗</a></div>
    </div>
    <dl class="facts">
      <div><dt>Card</dt><dd><span class="st c-muted"><span aria-hidden="true">○</span>Not registered</span></dd></div>
      <div><dt>GitHub access</dt><dd><span class="st c-{access.tone}"><span aria-hidden="true">{access.glyph}</span>{access.word}</span></dd></div>
      <div><dt>Automatic verification</dt><dd><span class="st c-{readiness.tone}"><span aria-hidden="true">{readiness.glyph}</span>{readiness.word}</span></dd></div>
    </dl>
    <section class="section" aria-labelledby="declare-heading">
      <div class="section-head"><h2 class="h" id="declare-heading">Declare it</h2><CopyButton label="Copy wyrd.toml" text={data.toml} /></div>
      <p class="prose">Add this file at the repository root, point verifier at your own Verifier definitions, then run:</p>
      <Code text={data.toml} toml />
      <div class="ident"><code class="mono">{COMMAND}</code><CopyButton label="Copy command" text={COMMAND} /></div>
      <p class="note">The server infers the Repository Card name, provider, host and repository_id, verifies them against this connection, registers the Card and selects that exact version.</p>
    </section>
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading the repository" />
  {:else if result.state !== 'empty'}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</RepositorySheet>
