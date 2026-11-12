<script lang="ts">
  // Text with highlighted matches: server snippets carry ⟦…⟧ markers; titles
  // are highlighted by the query words. Rendered as text nodes, never as HTML.
  let { text = '', words = [] as string[] }: { text?: string | null; words?: string[] } = $props();

  type Part = { t: string; hit: boolean };
  const parts = $derived.by<Part[]>(() => {
    const s = text ?? '';
    if (s.includes('⟦')) {
      const out: Part[] = [];
      for (const chunk of s.split('⟦')) {
        const i = chunk.indexOf('⟧');
        if (i < 0) out.push({ t: chunk, hit: false });
        else {
          out.push({ t: chunk.slice(0, i), hit: true });
          out.push({ t: chunk.slice(i + 1), hit: false });
        }
      }
      return out.filter((p) => p.t);
    }
    const ws = words.filter((w) => w.length >= 2).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
    if (!ws.length) return [{ t: s, hit: false }];
    const re = new RegExp(`(${ws.join('|')})`, 'giu');
    return s.split(re).map((t, i) => ({ t, hit: i % 2 === 1 })).filter((p) => p.t);
  });
</script>

{#each parts as p, i (i)}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}

<style>
  mark { background: var(--fill-bg, #fdecc8); color: inherit; border-radius: 2px; padding: 0 1px; box-shadow: 0 0 0 1px var(--fill-border, transparent); }
</style>
