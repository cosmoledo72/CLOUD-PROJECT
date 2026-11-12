<script lang="ts">
  // Visual editor of a knowledge base page («как в Word»): a toolbar on top,
  // a «/» menu for inserting blocks, images and files uploaded into the page
  // (also by pasting or dropping), links to sites and to other pages.
  import { onDestroy, onMount } from 'svelte';
  import { Editor } from '@tiptap/core';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Redo2 from '@lucide/svelte/icons/redo-2';
  import Bold from '@lucide/svelte/icons/bold';
  import Italic from '@lucide/svelte/icons/italic';
  import Underline from '@lucide/svelte/icons/underline';
  import Strikethrough from '@lucide/svelte/icons/strikethrough';
  import Baseline from '@lucide/svelte/icons/baseline';
  import Highlighter from '@lucide/svelte/icons/highlighter';
  import List from '@lucide/svelte/icons/list';
  import ListOrdered from '@lucide/svelte/icons/list-ordered';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import TableIcon from '@lucide/svelte/icons/table';
  import ImageIcon from '@lucide/svelte/icons/image';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import Link2 from '@lucide/svelte/icons/link-2';
  import FileText from '@lucide/svelte/icons/file-text';
  import Quote from '@lucide/svelte/icons/quote';
  import Code from '@lucide/svelte/icons/code';
  import Minus from '@lucide/svelte/icons/minus';
  import RemoveFormatting from '@lucide/svelte/icons/remove-formatting';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Heading1 from '@lucide/svelte/icons/heading-1';
  import Heading2 from '@lucide/svelte/icons/heading-2';
  import Heading3 from '@lucide/svelte/icons/heading-3';
  import Pilcrow from '@lucide/svelte/icons/pilcrow';
  import Modal from '$lib/Modal.svelte';
  import { api, type KbNode } from '$lib/api';
  import { kbExtensions, BG_COLORS, TEXT_COLORS } from './extensions';
  import './content.css';

  let {
    content,
    html = $bindable(''),
    pages = [],
    ensurePage,
    onsave
  }: {
    content: string;
    html?: string;
    pages?: KbNode[];
    /** Id of the page (creates a new page first if needed) — files are stored with it. */
    ensurePage: () => Promise<string>;
    /** Ctrl+S */
    onsave?: () => void;
  } = $props();

  let host = $state<HTMLDivElement>();
  let editor = $state<Editor | null>(null);
  // Re-read toolbar state after every transaction.
  let tick = $state(0);
  let error = $state('');
  let uploading = $state(0);

  onMount(() => {
    editor = new Editor({
      element: host!,
      extensions: kbExtensions(),
      content,
      editorProps: {
        attributes: { class: 'kbc pm', spellcheck: 'true' },
        handlePaste: (_view, event) => {
          const files = Array.from(event.clipboardData?.files ?? []);
          if (!files.length) return false;
          event.preventDefault();
          void insertFiles(files);
          return true;
        },
        handleDrop: (view, event, _slice, moved) => {
          const files = Array.from(event.dataTransfer?.files ?? []);
          if (moved || !files.length) return false;
          event.preventDefault();
          const pos = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
          void insertFiles(files, pos);
          return true;
        },
        handleKeyDown: (_view, event) => slashKey(event)
      },
      onTransaction: () => {
        tick++;
        updateSlash();
      },
      onUpdate: ({ editor: e }) => {
        html = e.getHTML();
      }
    });
    html = editor.getHTML();
    setTimeout(() => editor?.commands.focus('end'));
  });
  onDestroy(() => editor?.destroy());

  const is = (name: string, attrs?: Record<string, unknown>) => {
    void tick;
    return editor?.isActive(name, attrs) ?? false;
  };
  const canUndo = $derived.by(() => { void tick; return !!editor?.can().undo(); });
  const canRedo = $derived.by(() => { void tick; return !!editor?.can().redo(); });
  const chain = () => editor!.chain().focus();

  // --- Block type -------------------------------------------------------------
  const BLOCKS = [
    { k: 'p', label: 'Обычный текст', icon: Pilcrow, run: () => chain().setParagraph().run() },
    { k: 'h1', label: 'Заголовок 1', icon: Heading1, run: () => chain().toggleHeading({ level: 1 }).run() },
    { k: 'h2', label: 'Заголовок 2', icon: Heading2, run: () => chain().toggleHeading({ level: 2 }).run() },
    { k: 'h3', label: 'Заголовок 3', icon: Heading3, run: () => chain().toggleHeading({ level: 3 }).run() },
    { k: 'quote', label: 'Цитата', icon: Quote, run: () => chain().toggleBlockquote().run() },
    { k: 'code', label: 'Код', icon: Code, run: () => chain().toggleCodeBlock().run() }
  ];
  const blockNow = $derived.by(() => {
    void tick;
    if (!editor) return BLOCKS[0];
    for (const l of [1, 2, 3]) if (editor.isActive('heading', { level: l })) return BLOCKS[l];
    if (editor.isActive('blockquote')) return BLOCKS[4];
    if (editor.isActive('codeBlock')) return BLOCKS[5];
    return BLOCKS[0];
  });

  // --- Popovers (block type, colors, table) ---------------------------------------
  let pop = $state<'' | 'block' | 'color' | 'bg' | 'table'>('');
  function toggle(p: typeof pop) {
    pop = pop === p ? '' : p;
  }
  $effect(() => {
    if (!pop) return;
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== 'Escape') return;
      if (e instanceof PointerEvent && (e.target as HTMLElement).closest('.tbpop, .tbbtn.open')) return;
      pop = '';
    };
    const t = setTimeout(() => {
      document.addEventListener('pointerdown', close);
      document.addEventListener('keydown', close);
    });
    return () => {
      clearTimeout(t);
      document.removeEventListener('pointerdown', close);
      document.removeEventListener('keydown', close);
    };
  });
  const curColor = $derived.by(() => { void tick; return (editor?.getAttributes('textStyle').color as string | undefined) ?? ''; });
  const curBg = $derived.by(() => { void tick; return (editor?.getAttributes('textStyle').backgroundColor as string | undefined) ?? ''; });

  // --- Tables --------------------------------------------------------------------
  const inTable = $derived(is('table'));
  function insertTable() {
    chain().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run();
    pop = '';
  }

  // --- Files and images ---------------------------------------------------------------
  let imageInput = $state<HTMLInputElement>();
  let fileInput = $state<HTMLInputElement>();
  interface Uploaded { id: string; name: string; extension: string; size_bytes: number; url: string; image: boolean; author_name: string; created_at: string }

  async function upload(f: File): Promise<Uploaded> {
    const id = await ensurePage();
    const fd = new FormData();
    fd.append('file', f, f.name || 'image.png');
    return api.post<Uploaded>(`/kb/pages/${id}/files`, fd);
  }
  async function insertFiles(files: File[], pos?: number, asCard = false) {
    error = '';
    for (const f of files) {
      uploading++;
      try {
        const u = await upload(f);
        const node = u.image && !asCard
          ? { type: 'image', attrs: { src: u.url, alt: u.name } }
          : { type: 'fileCard', attrs: { id: u.id, name: u.name, size: String(u.size_bytes), ext: u.extension, author: u.author_name,
                                          date: new Date(u.created_at).toLocaleDateString('ru-RU') } };
        const c = editor!.chain().focus();
        (pos !== undefined ? c.insertContentAt(pos, node) : c.insertContent(node)).run();
        // Leave the cursor after the inserted block, so the next insert does not replace it.
        const after = editor!.state.selection.to;
        editor!.commands.setTextSelection(Math.min(after + 1, editor!.state.doc.content.size));
      } catch (e) {
        const st = (e as { status?: number }).status;
        error = st === 413 ? `«${f.name}»: файл больше 50 МБ или место в хранилище закончилось` : `«${f.name}»: ${(e as Error).message}`;
      } finally {
        uploading--;
      }
    }
  }

  // --- Links ------------------------------------------------------------------------------
  let linkOpen = $state(false);
  let linkUrl = $state('');
  function openLink() {
    linkUrl = (editor?.getAttributes('link').href as string | undefined) ?? '';
    linkOpen = true;
  }
  function applyLink() {
    const url = linkUrl.trim();
    if (!url) chain().extendMarkRange('link').unsetLink().run();
    else {
      const href = /^(https?:|mailto:|tel:|\/)/i.test(url) ? url : `https://${url}`;
      if (editor!.state.selection.empty && !editor!.isActive('link')) chain().insertContent({ type: 'text', text: url, marks: [{ type: 'link', attrs: { href } }] }).run();
      else chain().extendMarkRange('link').setLink({ href }).run();
    }
    linkOpen = false;
  }
  let pageLinkOpen = $state(false);
  let pageQ = $state('');
  const pageHits = $derived(pages.filter((p) => p.title.toLowerCase().includes(pageQ.trim().toLowerCase())).slice(0, 30));
  function insertPageLink(p: KbNode) {
    const text = editor!.state.selection.empty ? p.title : null;
    const c = chain();
    if (text) c.insertContent({ type: 'text', text, marks: [{ type: 'link', attrs: { href: `/kb?p=${p.id}` } }] }).insertContent(' ');
    else c.setLink({ href: `/kb?p=${p.id}` });
    c.run();
    pageLinkOpen = false;
    pageQ = '';
  }

  // --- «/» menu ---------------------------------------------------------------------------
  const SLASH = [
    { k: 'h1', label: 'Заголовок 1', words: 'заголовок h1', icon: Heading1, run: () => chain().setHeading({ level: 1 }).run() },
    { k: 'h2', label: 'Заголовок 2', words: 'заголовок h2', icon: Heading2, run: () => chain().setHeading({ level: 2 }).run() },
    { k: 'h3', label: 'Заголовок 3', words: 'заголовок h3', icon: Heading3, run: () => chain().setHeading({ level: 3 }).run() },
    { k: 'ul', label: 'Список', words: 'список маркированный', icon: List, run: () => chain().toggleBulletList().run() },
    { k: 'ol', label: 'Нумерованный список', words: 'нумерованный список', icon: ListOrdered, run: () => chain().toggleOrderedList().run() },
    { k: 'task', label: 'Чек-лист', words: 'чек-лист задачи галочки', icon: ListChecks, run: () => chain().toggleTaskList().run() },
    { k: 'table', label: 'Таблица', words: 'таблица', icon: TableIcon, run: () => insertTable() },
    { k: 'img', label: 'Изображение', words: 'изображение картинка фото', icon: ImageIcon, run: () => imageInput?.click() },
    { k: 'file', label: 'Файл (карточка)', words: 'файл вложение', icon: Paperclip, run: () => fileInput?.click() },
    { k: 'page', label: 'Ссылка на страницу', words: 'ссылка страница', icon: FileText, run: () => (pageLinkOpen = true) },
    { k: 'quote', label: 'Цитата / выделенный блок', words: 'цитата блок', icon: Quote, run: () => chain().toggleBlockquote().run() },
    { k: 'code', label: 'Код', words: 'код', icon: Code, run: () => chain().toggleCodeBlock().run() },
    { k: 'hr', label: 'Разделитель', words: 'разделитель линия', icon: Minus, run: () => chain().setHorizontalRule().run() }
  ];
  let slash = $state<{ from: number; to: number; q: string; x: number; y: number } | null>(null);
  let slashActive = $state(0);
  const slashItems = $derived(slash ? SLASH.filter((s) => !slash!.q || s.label.toLowerCase().includes(slash!.q) || s.words.includes(slash!.q)) : []);

  function updateSlash() {
    const e = editor;
    if (!e || !e.isFocused) {
      slash = null;
      return;
    }
    const sel = e.state.selection;
    const rp = sel.$from;
    const empty = sel.empty;
    if (!empty || rp.parent.type.name === 'codeBlock') {
      slash = null;
      return;
    }
    const before = rp.parent.textBetween(0, rp.parentOffset, undefined, '￼');
    const m = /(?:^|\s)\/([\p{L}\d-]{0,20})$/u.exec(before);
    if (!m) {
      slash = null;
      return;
    }
    const to = rp.pos;
    const from = to - m[1].length - 1;
    const c = e.view.coordsAtPos(from);
    const q = m[1].toLowerCase();
    if (!slash || slash.q !== q) slashActive = 0;
    slash = { from, to, q, x: c.left, y: c.bottom + 6 };
  }
  function runSlash(i: number) {
    const item = slashItems[i];
    if (!item || !slash) return;
    editor!.chain().focus().deleteRange({ from: slash.from, to: slash.to }).run();
    slash = null;
    item.run();
  }
  function slashKey(e: KeyboardEvent): boolean {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      onsave?.();
      return true;
    }
    if (!slash || !slashItems.length) return false;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      slashActive = (slashActive + (e.key === 'ArrowDown' ? 1 : slashItems.length - 1)) % slashItems.length;
      return true;
    }
    if (e.key === 'Enter') {
      runSlash(slashActive);
      return true;
    }
    if (e.key === 'Escape') {
      slash = null;
      return true;
    }
    return false;
  }
</script>

<div class="kbe">
  <div class="toolbar" role="toolbar" aria-label="Форматирование">
    <button class="tbbtn" title="Отменить (Ctrl+Z)" aria-label="Отменить" disabled={!canUndo} onclick={() => chain().undo().run()}><Undo2 size={16} /></button>
    <button class="tbbtn" title="Повторить (Ctrl+Y)" aria-label="Повторить" disabled={!canRedo} onclick={() => chain().redo().run()}><Redo2 size={16} /></button>
    <i class="sep"></i>
    <span class="wrap">
      <button class="tbbtn block" class:open={pop === 'block'} onclick={() => toggle('block')} aria-haspopup="menu" title="Стиль абзаца">{blockNow.label}<ChevronDown size={14} /></button>
      {#if pop === 'block'}
        <div class="tbpop menu" role="menu">
          {#each BLOCKS as b (b.k)}
            {@const Icon = b.icon}
            <button role="menuitem" class:on={blockNow.k === b.k} onclick={() => { b.run(); pop = ''; }}><Icon size={16} /><span class="bl-{b.k}">{b.label}</span></button>
          {/each}
        </div>
      {/if}
    </span>
    <i class="sep"></i>
    <button class="tbbtn" class:on={is('bold')} title="Жирный (Ctrl+B)" aria-label="Жирный" onclick={() => chain().toggleBold().run()}><Bold size={16} /></button>
    <button class="tbbtn" class:on={is('italic')} title="Курсив (Ctrl+I)" aria-label="Курсив" onclick={() => chain().toggleItalic().run()}><Italic size={16} /></button>
    <button class="tbbtn" class:on={is('underline')} title="Подчёркнутый (Ctrl+U)" aria-label="Подчёркнутый" onclick={() => chain().toggleUnderline().run()}><Underline size={16} /></button>
    <button class="tbbtn" class:on={is('strike')} title="Зачёркнутый" aria-label="Зачёркнутый" onclick={() => chain().toggleStrike().run()}><Strikethrough size={16} /></button>
    <span class="wrap">
      <button class="tbbtn col" class:open={pop === 'color'} title="Цвет текста" aria-label="Цвет текста" onclick={() => toggle('color')}>
        <Baseline size={16} /><span class="bar" style="background: {curColor || 'var(--text)'}"></span>
      </button>
      {#if pop === 'color'}
        <div class="tbpop swatches" role="menu" aria-label="Цвет текста">
          <button class="sw none" title="Обычный цвет" onclick={() => { chain().unsetColor().run(); pop = ''; }}>A</button>
          {#each TEXT_COLORS as c (c)}<button class="sw" class:on={curColor === c} style="color: {c}" title={c} onclick={() => { chain().setColor(c).run(); pop = ''; }}>A</button>{/each}
        </div>
      {/if}
    </span>
    <span class="wrap">
      <button class="tbbtn col" class:open={pop === 'bg'} title="Цвет фона" aria-label="Цвет фона" onclick={() => toggle('bg')}>
        <Highlighter size={16} /><span class="bar" style="background: {curBg || 'transparent'}"></span>
      </button>
      {#if pop === 'bg'}
        <div class="tbpop swatches" role="menu" aria-label="Цвет фона">
          <button class="sw none" title="Без фона" onclick={() => { chain().unsetBackgroundColor().run(); pop = ''; }}>✕</button>
          {#each BG_COLORS as c (c)}<button class="sw" class:on={curBg === c} style="background: {c}" title={c} aria-label="Фон {c}" onclick={() => { chain().setBackgroundColor(c).run(); pop = ''; }}></button>{/each}
        </div>
      {/if}
    </span>
    <i class="sep"></i>
    <button class="tbbtn" class:on={is('bulletList')} title="Список" aria-label="Список" onclick={() => chain().toggleBulletList().run()}><List size={16} /></button>
    <button class="tbbtn" class:on={is('orderedList')} title="Нумерованный список" aria-label="Нумерованный список" onclick={() => chain().toggleOrderedList().run()}><ListOrdered size={16} /></button>
    <button class="tbbtn" class:on={is('taskList')} title="Чек-лист" aria-label="Чек-лист" onclick={() => chain().toggleTaskList().run()}><ListChecks size={16} /></button>
    <i class="sep"></i>
    <span class="wrap">
      <button class="tbbtn" class:on={inTable} class:open={pop === 'table'} title="Таблица" aria-label="Таблица" onclick={() => (inTable ? toggle('table') : insertTable())}><TableIcon size={16} />{#if inTable}<ChevronDown size={13} />{/if}</button>
      {#if pop === 'table'}
        <div class="tbpop menu" role="menu">
          <button role="menuitem" onclick={() => { chain().addRowBefore().run(); pop = ''; }}>Строка выше</button>
          <button role="menuitem" onclick={() => { chain().addRowAfter().run(); pop = ''; }}>Строка ниже</button>
          <button role="menuitem" onclick={() => { chain().addColumnBefore().run(); pop = ''; }}>Столбец слева</button>
          <button role="menuitem" onclick={() => { chain().addColumnAfter().run(); pop = ''; }}>Столбец справа</button>
          <button role="menuitem" onclick={() => { chain().mergeOrSplit().run(); pop = ''; }}>Объединить / разделить ячейки</button>
          <button role="menuitem" onclick={() => { chain().toggleHeaderRow().run(); pop = ''; }}>Строка заголовка</button>
          <hr />
          <button role="menuitem" onclick={() => { chain().deleteRow().run(); pop = ''; }}>Удалить строку</button>
          <button role="menuitem" onclick={() => { chain().deleteColumn().run(); pop = ''; }}>Удалить столбец</button>
          <button role="menuitem" class="danger" onclick={() => { chain().deleteTable().run(); pop = ''; }}>Удалить таблицу</button>
        </div>
      {/if}
    </span>
    <button class="tbbtn" title="Изображение (можно вставить из буфера или перетащить)" aria-label="Изображение" onclick={() => imageInput?.click()}><ImageIcon size={16} /></button>
    <button class="tbbtn" title="Файл — карточка со ссылкой на скачивание" aria-label="Файл" onclick={() => fileInput?.click()}><Paperclip size={16} /></button>
    <button class="tbbtn" class:on={is('link')} title="Ссылка" aria-label="Ссылка" onclick={openLink}><Link2 size={16} /></button>
    <button class="tbbtn" title="Ссылка на страницу базы знаний" aria-label="Ссылка на страницу" onclick={() => (pageLinkOpen = true)}><FileText size={16} /></button>
    <i class="sep"></i>
    <button class="tbbtn" class:on={is('blockquote')} title="Цитата" aria-label="Цитата" onclick={() => chain().toggleBlockquote().run()}><Quote size={16} /></button>
    <button class="tbbtn" title="Разделитель" aria-label="Разделитель" onclick={() => chain().setHorizontalRule().run()}><Minus size={16} /></button>
    <button class="tbbtn" title="Очистить форматирование" aria-label="Очистить форматирование" onclick={() => chain().unsetAllMarks().clearNodes().run()}><RemoveFormatting size={16} /></button>
    {#if uploading}<span class="up">Загрузка: {uploading}…</span>{/if}
  </div>
  {#if error}<div class="error-box err">{error}</div>{/if}
  <div class="area" bind:this={host}></div>
  <input type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple hidden bind:this={imageInput}
         onchange={(e) => { const t = e.currentTarget; if (t.files) void insertFiles(Array.from(t.files)); t.value = ''; }} />
  <input type="file" multiple hidden bind:this={fileInput}
         onchange={(e) => { const t = e.currentTarget; if (t.files) void insertFiles(Array.from(t.files), undefined, true); t.value = ''; }} />
</div>

{#if slash && slashItems.length}
  <div class="slash" role="listbox" aria-label="Вставить" style="left: {Math.min(slash.x, innerWidth - 290)}px; top: {Math.min(slash.y, innerHeight - 360)}px">
    {#each slashItems as s, i (s.k)}
      {@const Icon = s.icon}
      <button role="option" aria-selected={i === slashActive} class:on={i === slashActive} onmousemove={() => (slashActive = i)}
              onmousedown={(e) => { e.preventDefault(); runSlash(i); }}><Icon size={16} />{s.label}</button>
    {/each}
  </div>
{/if}

<Modal bind:open={linkOpen} title="Ссылка" width={460}>
  <form id="kb-link" onsubmit={(e) => { e.preventDefault(); applyLink(); }}>
    <!-- svelte-ignore a11y_autofocus -->
    <input class="input" bind:value={linkUrl} placeholder="https://…" autofocus aria-label="Адрес" />
    <p class="faint small hint">Оставьте поле пустым, чтобы убрать ссылку. Для страницы базы знаний — кнопка «Ссылка на страницу».</p>
  </form>
  {#snippet footer()}
    <button class="btn" onclick={() => (linkOpen = false)}>Отмена</button>
    <button class="btn primary" form="kb-link">Готово</button>
  {/snippet}
</Modal>

<Modal bind:open={pageLinkOpen} title="Ссылка на страницу" width={480}>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="input" bind:value={pageQ} placeholder="Название страницы" autofocus aria-label="Найти страницу" />
  <ul class="plist">
    {#each pageHits as p (p.id)}
      <li><button onclick={() => insertPageLink(p)}><span>{p.icon || '📄'}</span>{p.title}</button></li>
    {:else}
      <li class="faint small empty">Ничего не найдено</li>
    {/each}
  </ul>
</Modal>

<style>
  .kbe { display: grid; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); }
  .toolbar { position: sticky; top: 0; z-index: 5; display: flex; flex-wrap: wrap; align-items: center; gap: 2px; padding: 6px 8px; border-bottom: 1px solid var(--border);
             background: var(--surface-2); border-radius: 10px 10px 0 0; }
  .tbbtn { display: inline-flex; align-items: center; gap: 3px; min-width: 32px; height: 32px; padding: 0 7px; border: 0; border-radius: 7px; background: none;
           color: var(--text-2); cursor: pointer; font: inherit; font-size: 13px; justify-content: center; }
  .tbbtn:hover:not(:disabled) { background: var(--surface-3); color: var(--text); }
  .tbbtn.on, .tbbtn.open { background: var(--accent-soft); color: var(--accent); }
  .tbbtn:disabled { opacity: .35; cursor: default; }
  .tbbtn.block { min-width: 150px; justify-content: space-between; border: 1px solid var(--border); background: var(--surface); }
  .tbbtn.col { flex-direction: column; gap: 1px; padding-top: 3px; }
  .bar { width: 16px; height: 3px; border-radius: 2px; border: 1px solid var(--border); }
  .sep { width: 1px; height: 20px; background: var(--border); margin: 0 4px; }
  .wrap { position: relative; display: inline-flex; }
  .tbpop { position: absolute; top: calc(100% + 6px); left: 0; z-index: 30; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-lg); padding: 6px; }
  .menu { width: 240px; display: grid; gap: 1px; }
  .menu button { display: flex; align-items: center; gap: 10px; padding: 7px 10px; border: 0; border-radius: 7px; background: none; font: inherit; font-size: 13.5px; color: var(--text); cursor: pointer; text-align: left; }
  .menu button:hover, .menu button.on { background: var(--accent-soft); }
  .menu button.danger { color: var(--danger); }
  .menu hr { border: 0; border-top: 1px solid var(--border); margin: 4px 0; }
  .bl-h1 { font-size: 18px; font-weight: 700; }
  .bl-h2 { font-size: 16px; font-weight: 700; }
  .bl-h3 { font-size: 14.5px; font-weight: 600; }
  .swatches { display: grid; grid-template-columns: repeat(4, 30px); gap: 6px; }
  .sw { width: 30px; height: 30px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); font-weight: 700; font-size: 15px; cursor: pointer; }
  .sw.on { outline: 2px solid var(--accent); }
  .sw.none { color: var(--text-3); font-weight: 500; }
  .up { margin-left: auto; font-size: 12.5px; color: var(--text-3); padding-right: 6px; }
  .err { margin: 8px 12px 0; }
  .area { min-height: 420px; padding: 18px 26px 26px; cursor: text; }
  .area :global(.pm) { outline: none; min-height: 380px; }
  .slash { position: fixed; z-index: 80; width: 280px; max-height: 340px; overflow-y: auto; background: var(--surface); border: 1px solid var(--border); border-radius: 10px;
           box-shadow: var(--shadow-lg); padding: 6px; display: grid; gap: 1px; }
  .slash button { display: flex; align-items: center; gap: 10px; padding: 7px 10px; border: 0; border-radius: 7px; background: none; font: inherit; font-size: 13.5px; color: var(--text); cursor: pointer; text-align: left; }
  .slash button.on { background: var(--accent-soft); }
  .slash button :global(svg) { color: var(--text-3); }
  .hint { margin: 8px 0 0; }
  .plist { list-style: none; margin: 10px 0 0; padding: 0; max-height: 340px; overflow-y: auto; border: 1px solid var(--border); border-radius: var(--radius); }
  .plist li + li { border-top: 1px solid var(--border); }
  .plist button { display: flex; gap: 10px; width: 100%; padding: 8px 10px; border: 0; background: none; font: inherit; color: var(--text); cursor: pointer; text-align: left; }
  .plist button:hover { background: var(--accent-soft); }
  .empty { padding: 14px; text-align: center; }
  @media (max-width: 720px) {
    .area { padding: 14px; }
    .tbbtn.block { min-width: 120px; }
  }
</style>
