<script lang="ts">
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';
  import SquareCheckBig from '@lucide/svelte/icons/square-check-big';
  import BookOpen from '@lucide/svelte/icons/book-open';
  import Building from '@lucide/svelte/icons/building';
  import Contact from '@lucide/svelte/icons/contact';
  import HandCoins from '@lucide/svelte/icons/hand-coins';
  import MessageSquareText from '@lucide/svelte/icons/message-square-text';
  import MessagesSquare from '@lucide/svelte/icons/messages-square';
  import UserRound from '@lucide/svelte/icons/user-round';
  import FileIcon from '$lib/FileIcon.svelte';
  import type { Hit } from './search.svelte';

  let { hit }: { hit: Hit } = $props();
  const ICONS = { projects: FolderKanban, tasks: SquareCheckBig, kb: BookOpen, companies: Building, contacts: Contact, deals: HandCoins,
                  comments: MessageSquareText, messages: MessagesSquare, people: UserRound };
</script>

<span class="hi" class:done={hit.type === 'tasks' && hit.status === 'done'}>
  {#if hit.type === 'files'}
    <FileIcon ext={hit.ext ?? ''} />
  {:else if hit.type === 'kb' && hit.icon}
    <span class="emoji">{hit.icon}</span>
  {:else}
    {@const Icon = ICONS[hit.type]}
    <Icon size={17} strokeWidth={1.8} />
  {/if}
</span>

<style>
  .hi { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 7px; background: var(--surface-3); color: var(--text-2); flex: none; }
  .hi.done { color: var(--success, #2f9e5b); }
  .emoji { font-size: 16px; line-height: 1; }
</style>
