<script lang="ts">
  // Read-only page content. The server already cleans the HTML; it is
  // cleaned again here. On a public page, file addresses are rewritten to
  // the link's own (`fileBase`).
  import DOMPurify from 'dompurify';
  import './content.css';

  let { html, fileBase = null, fileQuery = '' }: { html: string; fileBase?: string | null; fileQuery?: string } = $props();

  const clean = $derived.by(() => {
    let out = DOMPurify.sanitize(html, { ADD_ATTR: ['target', 'colwidth', 'data-checked', 'data-type'], FORBID_TAGS: ['style', 'form'] });
    if (fileBase) out = out.replace(/\/api\/kb\/files\/([0-9a-f-]{36})(\?[^"']*)?/g, (_m, id: string) => `${fileBase}${id}${fileQuery ? `?${fileQuery}` : ''}`);
    // Task checkboxes are not clickable when reading.
    return out.replaceAll('<input type="checkbox"', '<input type="checkbox" disabled');
  });
</script>

<div class="kbc view">{@html clean}</div>

<style>
  .view :global(li[data-type='taskItem'] input) { cursor: default; }
</style>
