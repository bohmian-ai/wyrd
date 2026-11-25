<!--
  An administration drawer or confirmation dialog whose open state lives in
  the URL: the page renders it when its query parameter is present, and the
  backdrop, the close control and Escape all go to `close`, which writes
  nothing. It is a native modal <dialog>, so the page behind it is inert;
  Tab and Shift+Tab wrap at its ends rather than leaving the document, and
  closing returns focus to whatever opened it. Styled by console.css.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { goto } from '$app/navigation';

  interface Props {
    kind: 'drawer' | 'dialog';
    /** The accessible name, and the drawer's heading. */
    title: string;
    /** Where closing goes; the page without this overlay's parameter. */
    close: string;
    /** A drawer heading richer than its title. */
    heading?: Snippet;
    children: Snippet;
    footer?: Snippet;
  }
  let { kind, title, close, heading, children, footer }: Props = $props();
  const uid = $props.id();

  const leave = () => goto(close, { noScroll: true, keepFocus: true });

  /**
   * Open modally and start focus on the overlay itself. Unmounting closes it
   * first, which hands focus back to the opener while the page is no longer
   * inert.
   */
  function modal(node: HTMLDialogElement) {
    node.showModal();
    node.focus();
    return { destroy: () => node.close() };
  }
  /** Escape: let the URL close it, so nothing closes without navigating. */
  function oncancel(event: Event) {
    event.preventDefault();
    leave();
  }
  const STOPS = 'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex="0"]';
  /** Wrap Tab at the overlay's last stop and Shift+Tab at its first, or from the overlay itself. */
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== 'Tab') return;
    const node = event.currentTarget as HTMLDialogElement;
    const stops = [...node.querySelectorAll<HTMLElement>(STOPS)].filter((stop) => stop.checkVisibility());
    if (!stops.length) return;
    const [first, last] = [stops[0], stops[stops.length - 1]];
    const at = document.activeElement;
    if (event.shiftKey ? at === first || at === node : at === last) {
      event.preventDefault();
      (event.shiftKey ? last : first).focus();
    }
  }
  /** A backdrop click lands on the dialog itself but outside its box. */
  function onclick(event: MouseEvent) {
    const node = event.currentTarget as HTMLDialogElement;
    const box = node.getBoundingClientRect();
    if (event.target === node && (event.clientX < box.left || event.clientX > box.right || event.clientY < box.top || event.clientY > box.bottom)) leave();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -- Escape is the keyboard equivalent. -->
{#if kind === 'drawer'}
  <dialog class="drawer" aria-label={title} tabindex="-1" use:modal {oncancel} {onclick} {onkeydown}>
    <header>
      {#if heading}{@render heading()}{:else}<h2>{title}</h2>{/if}
      <a class="iconbtn" href={close} aria-label="Close" data-sveltekit-noscroll data-sveltekit-keepfocus>✕</a>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </dialog>
{:else}
  <dialog class="dialog" aria-labelledby="{uid}-title" tabindex="-1" use:modal {oncancel} {onclick} {onkeydown}>
    <div class="body">
      <h2 id="{uid}-title">{title}</h2>
      {@render children()}
    </div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </dialog>
{/if}
