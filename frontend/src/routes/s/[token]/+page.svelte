<script lang="ts">
  // Page behind a public link: a file, a folder (with subfolders) or a
  // knowledge base page, opened without an account. Password-protected links
  // ask for the password first; the access key is kept for this browser tab.
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import Download from '@lucide/svelte/icons/download';
  import Eye from '@lucide/svelte/icons/eye';
  import Lock from '@lucide/svelte/icons/lock';
  import LinkOff from '@lucide/svelte/icons/unlink';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Archive from '@lucide/svelte/icons/archive';
  import KbView from '$lib/kb/KbView.svelte';
  import { api, docKind } from '$lib/api';
  import FileIcon from '$lib/FileIcon.svelte';
  import { itemsLabel, plural, relTime, sizeLabel } from '$lib/files/fmt';

  interface PubFile { id: string; name: string; extension: string; size_bytes: number; updated_at: string }
  interface Info {
    type: 'file' | 'folder' | 'kb_page';
    name?: string;
    brand: { name: string; logo_url: string | null; accent: string };
    shared_by: string | null;
    expires_on: string | null;
    locked: boolean;
    file?: PubFile;
    folder?: {
      id: string;
      crumbs: { id: string; name: string }[];
      folders: { id: string; name: string; items: number }[];
      files: PubFile[];
      total_files: number;
      total_size: number;
    };
    page?: { title: string; icon: string; content: string; updated_at: string };
  }

  const token = $derived(page.params.token ?? '');
  const sub = $derived(page.url.searchParams.get('folder'));
  let info = $state<Info | null>(null);
  let dead = $state(false);
  let error = $state('');
  let k = $state('');
  let password = $state('');
  let wrong = $state(false);
  let busy = $state(false);

  function keyStore(t: string) {
    return `wh_link_${t}`;
  }
  $effect(() => {
    try { k = sessionStorage.getItem(keyStore(token)) ?? ''; } catch { k = ''; }
  });

  async function load() {
    error = '';
    const q = new URLSearchParams();
    if (k) q.set('k', k);
    if (sub) q.set('folder', sub);
    try {
      info = await api.get<Info>(`/public/s/${encodeURIComponent(token)}?${q}`);
      document.title = info.locked ? 'Документы по ссылке' : `${info.name ?? 'Документы'} — ${info.brand.name}`;
    } catch (e) {
      if ((e as { status?: number }).status === 404) dead = true;
      else error = (e as Error).message;
    }
  }
  $effect(() => {
    token;
    sub;
    k;
    load();
  });

  async function unlock(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    wrong = false;
    try {
      const r = await api.post<{ k: string | null }>(`/public/s/${encodeURIComponent(token)}/unlock`, { password });
      k = r.k ?? '';
      try { sessionStorage.setItem(keyStore(token), k); } catch { /* storage unavailable */ }
    } catch (err) {
      const st = (err as { status?: number }).status;
      if (st === 401) wrong = true;
      else error = st === 429 ? 'Слишком много попыток. Попробуйте через 15 минут.' : (err as Error).message;
    } finally {
      busy = false;
    }
  }

  const kq = $derived(k ? `?k=${encodeURIComponent(k)}` : '');
  const dl = (f: PubFile) => `/api/public/s/${encodeURIComponent(token)}/download/${f.id}${kq}`;
  const view = (f: PubFile) => `/s/${encodeURIComponent(token)}/view/${f.id}${kq}`;
  const zipUrl = $derived(`/api/public/s/${encodeURIComponent(token)}/zip?${new URLSearchParams({ ...(k ? { k } : {}), ...(sub ? { folder: sub } : {}) })}`);
  const viewable = (f: PubFile) => docKind(f.extension) !== 'other';
  const go = (id: string | null) => goto(id && id !== info?.folder?.crumbs[0]?.id ? `?folder=${id}` : '?', { keepFocus: true });
  const fmtDate = (d: string) => new Date(d + 'T00:00').toLocaleDateString('ru-RU', { day: '2-digit', month: '2-digit', year: 'numeric' });
</script>

<svelte:head><meta name="robots" content="noindex, nofollow" /></svelte:head>

<div class="pub">
  <header class="top">
    {#if info?.brand.logo_url}<img class="logo" src={info.brand.logo_url} alt="" />{/if}
    <b class="brand">{info?.brand.name ?? ''}</b>
    <span class="spacer"></span>
    {#if info?.expires_on && !info.locked}<span class="exp">Доступ по {fmtDate(info.expires_on)}</span>{/if}
  </header>

  <main class="body">
    {#if dead}
      <div class="card center">
        <LinkOff size={36} />
        <h1>Ссылка не работает</h1>
        <p class="muted">Срок её действия истёк, её отключили или в адресе ошибка. Попросите прислать новую ссылку.</p>
      </div>
    {:else if error}
      <div class="error-box">{error}</div>
    {:else if info?.locked}
      <form class="card center lock" onsubmit={unlock}>
        <Lock size={32} />
        <h1>Документы защищены паролем</h1>
        <p class="muted">{info.shared_by ? `${info.shared_by} передал(а) вам документы.` : ''} Введите пароль, который вам сообщили вместе со ссылкой.</p>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" type="password" bind:value={password} placeholder="Пароль" autofocus autocomplete="off" aria-label="Пароль" />
        {#if wrong}<div class="error-box">Пароль не подошёл</div>{/if}
        <button class="btn primary" disabled={busy || !password}>{busy ? 'Проверяю…' : 'Открыть'}</button>
      </form>
    {:else if info}
      {#if info.type === 'kb_page' && info.page}
        <article class="card kb">
          <h1>{#if info.page.icon}<span class="pi">{info.page.icon}</span>{/if}{info.page.title}</h1>
          <p class="meta">Обновлено {relTime(info.page.updated_at)}{info.shared_by ? ` · поделился(ась) ${info.shared_by}` : ''}</p>
          <KbView html={info.page.content} fileBase={`/api/public/s/${encodeURIComponent(token)}/kbfile/`} fileQuery={k ? `k=${encodeURIComponent(k)}` : ''} />
        </article>
      {:else}
        <div class="title">
          <h1>{info.name}</h1>
          <p class="meta">
            {#if info.shared_by}Передал(а): {info.shared_by}{/if}
            {#if info.folder} · {info.folder.total_files} {plural(info.folder.total_files, 'файл', 'файла', 'файлов')} · {sizeLabel(info.folder.total_size)}{/if}
          </p>
        </div>
        {#if info.file}
          {@const f = info.file}
          <div class="card one">
            <FileIcon ext={f.extension} />
            <div class="tx"><b>{f.name}</b><span class="meta">{sizeLabel(f.size_bytes)} · изменён {relTime(f.updated_at)}</span></div>
            {#if viewable(f)}<a class="btn" href={view(f)} target="_blank" rel="noopener"><Eye size={16} />Открыть</a>{/if}
            <a class="btn primary" href={dl(f)}><Download size={16} />Скачать</a>
          </div>
        {:else if info.folder}
          {@const d = info.folder}
          {#if d.crumbs.length > 1}
            <nav class="crumbs" aria-label="Путь">
              {#each d.crumbs as c, i (c.id)}
                {#if i}<ChevronRight size={14} />{/if}
                {#if i < d.crumbs.length - 1}<button class="linkbtn" onclick={() => go(c.id)}>{c.name}</button>{:else}<b>{c.name}</b>{/if}
              {/each}
            </nav>
          {/if}
          <div class="card list">
            <table>
              <thead><tr><th>Название</th><th class="c-size">Размер</th><th class="c-date">Изменён</th><th class="c-act"></th></tr></thead>
              <tbody>
                {#each d.folders as f (f.id)}
                  <tr class="dir" onclick={() => go(f.id)}>
                    <td><button class="name linkbtn" onclick={(e) => { e.stopPropagation(); go(f.id); }}><FileIcon folder /><span>{f.name}</span></button></td>
                    <td class="c-size muted">{itemsLabel(f.items)}</td><td class="c-date"></td><td class="c-act"></td>
                  </tr>
                {/each}
                {#each d.files as f (f.id)}
                  <tr>
                    <td><span class="name"><FileIcon ext={f.extension} /><span>{f.name}</span></span><span class="sub">{sizeLabel(f.size_bytes)} · {relTime(f.updated_at)}</span></td>
                    <td class="c-size muted">{sizeLabel(f.size_bytes)}</td>
                    <td class="c-date muted">{relTime(f.updated_at)}</td>
                    <td class="c-act">
                      {#if viewable(f)}<a class="btn sm" href={view(f)} target="_blank" rel="noopener" aria-label="Открыть {f.name}"><Eye size={15} /><span class="lbl">Открыть</span></a>{/if}
                      <a class="btn sm" href={dl(f)} aria-label="Скачать {f.name}"><Download size={15} /><span class="lbl">Скачать</span></a>
                    </td>
                  </tr>
                {/each}
                {#if !d.folders.length && !d.files.length}<tr><td colspan="4" class="empty muted">Папка пуста</td></tr>{/if}
              </tbody>
            </table>
          </div>
          {#if d.files.length || d.folders.length}
            <div><a class="btn primary" href={zipUrl}><Archive size={16} />Скачать {d.crumbs.length > 1 ? 'эту папку' : 'всё'} архивом (ZIP)</a></div>
          {/if}
        {/if}
      {/if}
    {/if}
  </main>
  <footer class="foot">Файлы переданы через портал {info?.brand.name ?? ''}. Ссылка личная — не пересылайте её посторонним.</footer>
</div>

<style>
  .pub { min-height: 100vh; background: var(--bg); color: var(--text); display: flex; flex-direction: column; }
  .top { display: flex; align-items: center; gap: 12px; padding: 12px 24px; background: var(--surface); border-bottom: 1px solid var(--border); }
  .logo { height: 32px; max-width: 160px; object-fit: contain; }
  .brand { font-size: 15px; }
  .spacer { flex: 1; }
  .exp { font-size: 13px; color: var(--text-3); }
  .body { flex: 1; width: min(1000px, 100%); margin: 0 auto; padding: 24px 16px; display: grid; gap: 14px; align-content: start; }
  h1 { margin: 0; font-size: 24px; }
  .meta { margin: 4px 0 0; font-size: 13px; color: var(--text-3); }
  .card { background: var(--surface); border: 1px solid var(--border); border-radius: 12px; }
  .center { padding: 32px 24px; display: grid; gap: 12px; justify-items: center; text-align: center; color: var(--text-2); max-width: 460px; margin: 6vh auto 0; width: 100%; }
  .center p { margin: 0; }
  .lock .input { width: 100%; max-width: 300px; }
  .one { display: flex; align-items: center; gap: 14px; padding: 16px 18px; flex-wrap: wrap; }
  .one .tx { flex: 1; min-width: 200px; display: grid; gap: 2px; }
  .btn { display: inline-flex; align-items: center; gap: 6px; text-decoration: none; }
  .crumbs { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; font-size: 13.5px; color: var(--text-2); }
  .linkbtn { border: 0; background: none; padding: 0; font: inherit; color: var(--accent); cursor: pointer; text-align: left; }
  .list { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; }
  th { text-align: left; font-weight: 500; font-size: 13px; color: var(--text-3); padding: 10px 14px; border-bottom: 1px solid var(--border); background: var(--surface-2); }
  td { padding: 9px 14px; border-bottom: 1px solid var(--border); vertical-align: middle; }
  tbody tr:last-child td { border-bottom: 0; }
  tbody tr:hover td { background: var(--surface-2); }
  tr.dir { cursor: pointer; }
  .name { display: inline-flex; align-items: center; gap: 10px; font-weight: 500; color: var(--text); }
  .linkbtn.name { color: var(--text); }
  .c-size { width: 110px; white-space: nowrap; }
  .c-date { width: 150px; white-space: nowrap; }
  .c-act { width: 1%; white-space: nowrap; text-align: right; }
  .c-act .btn + .btn { margin-left: 6px; }
  .sub { display: none; font-size: 12px; color: var(--text-3); margin: 2px 0 0 34px; }
  .empty { text-align: center; padding: 20px; }
  .kb { padding: 24px 28px; }
  .pi { margin-right: 8px; }
  .foot { text-align: center; font-size: 12px; color: var(--text-3); padding: 16px; }
  @media (max-width: 640px) {
    .c-size, .c-date { display: none; }
    .sub { display: block; }
    .lbl { display: none; }
    .top { padding: 10px 16px; }
    .kb { padding: 18px; }
  }
</style>
