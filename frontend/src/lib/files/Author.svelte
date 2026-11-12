<script lang="ts">
  // Author chip: avatar with initials and the name.
  import Avatar from '$lib/Avatar.svelte';
  let { id, name }: { id: string; name: string } = $props();
</script>

<span class="who" title={name}><Avatar {id} {name} size={24} /><span class="n">{name}</span></span>

<style>
  .who { display: inline-flex; align-items: center; gap: 7px; max-width: 100%; padding: 2px 10px 2px 2px; border-radius: 999px; background: var(--surface-3); font-size: 13px; color: var(--text); }
  .n { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
</style>
