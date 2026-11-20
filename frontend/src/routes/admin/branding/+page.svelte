<script lang="ts">
  import { api, upload } from '#lib/api.ts';
  import { applyBranding, branding, DEFAULT_ACCENT, FONT_SIZES, FONTS, loadFont, LOGIN_BASES, loginHero, type Branding, type FontKey } from '#lib/branding.svelte.ts';
  import { contrast, isHex, LIGHT_TEXT, textOn } from '#lib/color.ts';

  type Editable = Pick<Branding, 'display_name' | 'accent_color' | 'sidebar_style' | 'color_scheme' | 'corner_style' | 'login_title' | 'login_subtitle'
    | 'logo_plate' | 'login_logo_plate' | 'login_bg_overlay' | 'login_bg_tone' | 'login_bg_base' | 'font_family' | 'font_size'>;
  const EDITABLE: (keyof Editable)[] = ['display_name', 'accent_color', 'sidebar_style', 'color_scheme', 'corner_style', 'login_title', 'login_subtitle',
    'logo_plate', 'login_logo_plate', 'login_bg_overlay', 'login_bg_tone', 'login_bg_base', 'font_family', 'font_size'];
  const DEFAULTS: Editable = {
    display_name: '', accent_color: DEFAULT_ACCENT, sidebar_style: 'light', color_scheme: 'auto', corner_style: 'default', login_title: '', login_subtitle: '',
    logo_plate: false, login_logo_plate: true, login_bg_overlay: 45, login_bg_tone: 'dark', login_bg_base: 'brand', font_family: 'inter', font_size: 'default'
  };


  let saved = $state<Branding | null>(null);
  let draft = $state<Editable>({ ...DEFAULTS });
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);

  // Login panel base: a light base suggests the light tone (dark text), the brand gradient — the dark one.
  const customBase = $derived(draft.login_bg_base.startsWith('#'));
  function setBase(key: string) {
    draft.login_bg_base = key;
    if (key === 'light' || key === 'white') draft.login_bg_tone = 'light';
    else if (key === 'brand') draft.login_bg_tone = 'dark';
  }
  function setCustomBase(hex: string) {
    draft.login_bg_base = hex.toLowerCase();
    // Pick the tone that keeps the text readable on this colour.
    draft.login_bg_tone = textOn(hex) === LIGHT_TEXT ? 'dark' : 'light';
  }

  function take(b: Branding) {
    saved = b;
    draft = Object.fromEntries(EDITABLE.map((k) => [k, b[k]])) as Editable;
  }
  $effect(() => {
    api.get<Branding>('/branding').then(take).catch((e) => (error = e.message));
  });

  const dirty = $derived(!!saved && EDITABLE.some((k) => saved![k] !== draft[k]));
  let hexInput = $state('');
  $effect(() => {
    hexInput = draft.accent_color;
  });

  const swatches = [
    { c: '#1f4e79', n: 'Синий' }, { c: '#8b1e2d', n: 'Бордовый' }, { c: '#1e6b45', n: 'Зелёный' },
    { c: '#0f6e6e', n: 'Бирюзовый' }, { c: '#b45309', n: 'Охра' }, { c: '#c2410c', n: 'Терракота' },
    { c: '#5b3b8c', n: 'Фиолетовый' }, { c: '#334155', n: 'Графит' }
  ];

  // Accessibility of the chosen accent.
  const onWhite = $derived(isHex(draft.accent_color) ? contrast(draft.accent_color, '#ffffff') : 21);
  const onButton = $derived(isHex(draft.accent_color) ? contrast(draft.accent_color, textOn(draft.accent_color)) : 21);

  // --- Live preview -----------------------------------------------------------
  let previewEl = $state<HTMLElement>();
  let previewTheme = $state<'light' | 'dark'>('light');
  let previewMode = $state<'app' | 'login'>('app');
  $effect(() => {
    if (draft.color_scheme !== 'auto') previewTheme = draft.color_scheme;
  });
  $effect(() => {
    if (!previewEl) return;
    applyBranding(previewEl, {
      accent_color: isHex(draft.accent_color) ? draft.accent_color : DEFAULT_ACCENT, color_scheme: previewTheme, corner_style: draft.corner_style,
      font_family: draft.font_family, font_size: draft.font_size
    });
  });
  const previewName = $derived(draft.display_name.trim() || saved?.display_name || 'Портал');

  // --- Actions -------------------------------------------------------------------
  async function guarded(fn: () => Promise<void>, ok: string) {
    error = '';
    notice = '';
    busy = true;
    try {
      await fn();
      notice = ok;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  const save = () => guarded(async () => {
    const b = await api.put<Branding>('/admin/branding', draft);
    take(b);
    branding.set(b);
  }, 'Оформление сохранено и применено.');

  function resetDefaults() {
    draft = { ...DEFAULTS };
  }

  type AssetKind = 'logo' | 'icon' | 'login_bg';
  const ASSET_DONE: Record<AssetKind, [string, string]> = {
    logo: ['Логотип загружен.', 'Логотип удалён.'],
    icon: ['Значок загружен.', 'Значок удалён.'],
    login_bg: ['Фон страницы входа загружен.', 'Фон страницы входа удалён.']
  };
  async function onAsset(kind: AssetKind, e: Event & { currentTarget: HTMLInputElement }) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!file) return;
    const limit = kind === 'login_bg' ? 3 * 1024 * 1024 : 512 * 1024;
    if (file.size > limit) {
      error = kind === 'login_bg'
        ? 'Файл больше 3 МБ. Сохраните фото в JPEG или WebP с шириной около 1600–2000 px.'
        : 'Файл больше 512 КБ. Уменьшите изображение или используйте SVG.';
      return;
    }
    await guarded(async () => {
      // Only `saved` is replaced, so unsaved colour/text edits in `draft` survive the upload.
      const b = await upload<Branding>(`/admin/branding/asset?kind=${kind}`, file);
      saved = b;
      branding.set(b);
    }, ASSET_DONE[kind][0]);
  }
  const removeAsset = (kind: AssetKind) => guarded(async () => {
    const b = await api.del<Branding>(`/admin/branding/asset?kind=${kind}`);
    saved = b;
    branding.set(b);
  }, ASSET_DONE[kind][1]);

  const loginLink = $derived(saved ? `${location.origin}/login?portal=${saved.tenant_slug}` : '');
  const logoUrl = $derived(saved?.logo_url ?? null);
  const iconUrl = $derived(saved?.icon_url ?? null);
  const bgUrl = $derived(saved?.login_bg_url ?? null);

  // Show every font in its own face in the picker.
  $effect(() => {
    for (const k of Object.keys(FONTS) as FontKey[]) loadFont(k);
  });
</script>

{#if error}<div class="error-box">{error}</div>{/if}
{#if notice}<div class="ok-box">{notice}</div>{/if}

<div class="layout">
  <div class="form">
    <section class="card sec">
      <h2>Название и логотип</h2>
      <label class="field">Название портала в интерфейсе
        <input class="input" bind:value={draft.display_name} placeholder={saved?.display_name} maxlength="120" />
        <span class="faint small">Если пусто — используется название организации.</span>
      </label>

      <div class="assets">
        <div class="asset">
          <div class="asset-head"><b>Логотип</b><span class="faint small">PNG, SVG, JPEG, WebP · до 512 КБ · горизонтальный, высота от 80 px</span></div>
          <div class="asset-body">
            <div class="thumb wide">{#if logoUrl}<img src={logoUrl} alt="Логотип" />{:else}<span class="faint small">нет</span>{/if}</div>
            <div class="row">
              <label class="btn sm">Загрузить<input type="file" accept="image/png,image/jpeg,image/webp,image/svg+xml" hidden onchange={(e) => onAsset('logo', e)} /></label>
              {#if logoUrl}<button class="btn ghost sm danger" onclick={() => removeAsset('logo')}>Удалить</button>{/if}
            </div>
          </div>
        </div>
        <div class="asset">
          <div class="asset-head"><b>Значок</b><span class="faint small">Квадратный, от 64×64 · вкладка браузера</span></div>
          <div class="asset-body">
            <div class="thumb square">{#if iconUrl}<img src={iconUrl} alt="Значок" />{:else}<span class="faint small">нет</span>{/if}</div>
            <div class="row">
              <label class="btn sm">Загрузить<input type="file" accept="image/png,image/jpeg,image/webp,image/svg+xml" hidden onchange={(e) => onAsset('icon', e)} /></label>
              {#if iconUrl}<button class="btn ghost sm danger" onclick={() => removeAsset('icon')}>Удалить</button>{/if}
            </div>
          </div>
        </div>
      </div>
      {#if logoUrl}
        <label class="check"><input type="checkbox" bind:checked={draft.logo_plate} />Белая подложка под логотипом в боковой панели</label>
        {#if draft.sidebar_style !== 'light' && !draft.logo_plate}
          <p class="warn-note">Боковая панель тёмная или цветная: если логотип тёмный, включите белую подложку, иначе он будет плохо виден.</p>
        {/if}
      {/if}
      <p class="faint small">Логотип и значок применяются сразу после загрузки; остальные настройки — после «Сохранить».</p>
    </section>

    <section class="card sec">
      <h2>Фирменный цвет</h2>
      <div class="swatches" role="radiogroup" aria-label="Готовые цвета">
        {#each swatches as s}
          <button class="swatch" class:on={draft.accent_color === s.c} style="background:{s.c}" title={s.n} aria-label={s.n}
                  role="radio" aria-checked={draft.accent_color === s.c} onclick={() => (draft.accent_color = s.c)}></button>
        {/each}
      </div>
      <div class="row color-row">
        <input type="color" class="picker" value={isHex(draft.accent_color) ? draft.accent_color : DEFAULT_ACCENT}
               oninput={(e) => (draft.accent_color = e.currentTarget.value)} aria-label="Выбор цвета" />
        <input class="input mono hex" bind:value={hexInput} maxlength="7" aria-label="HEX"
               onchange={() => { const v = hexInput.startsWith('#') ? hexInput : '#' + hexInput; if (isHex(v)) draft.accent_color = v.toLowerCase(); else hexInput = draft.accent_color; }} />
        <span class="faint small">Контраст: на белом {onWhite.toFixed(1)}:1 · текст на кнопке {onButton.toFixed(1)}:1</span>
      </div>
      {#if onWhite < 3}
        <p class="warn-note">Цвет слишком светлый: ссылки и активные пункты будут плохо читаться на белом фоне. Выберите более тёмный оттенок.</p>
      {/if}
    </section>

    <section class="card sec">
      <h2>Интерфейс</h2>
      <div class="opt-group">
        <span class="opt-label">Боковая панель</span>
        <div class="opts">
          {#each [['light', 'Светлая'], ['dark', 'Тёмная'], ['accent', 'Фирменный цвет']] as [v, l]}
            <label class="opt" class:on={draft.sidebar_style === v}>
              <input type="radio" name="sidebar" value={v} bind:group={draft.sidebar_style} />
              <span class="mini-side {v}" style="--sw:{draft.accent_color}"></span>{l}
            </label>
          {/each}
        </div>
      </div>
      <div class="opt-group">
        <span class="opt-label">Цветовая схема</span>
        <div class="opts">
          {#each [['auto', 'Как в системе'], ['light', 'Всегда светлая'], ['dark', 'Всегда тёмная']] as [v, l]}
            <label class="opt" class:on={draft.color_scheme === v}><input type="radio" name="scheme" value={v} bind:group={draft.color_scheme} />{l}</label>
          {/each}
        </div>
      </div>
      <div class="opt-group">
        <span class="opt-label">Скругление углов</span>
        <div class="opts">
          {#each [['sharp', 'Строгие', 2], ['default', 'Стандартные', 6], ['round', 'Мягкие', 12]] as [v, l, r]}
            <label class="opt" class:on={draft.corner_style === v}>
              <input type="radio" name="corners" value={v} bind:group={draft.corner_style} />
              <span class="mini-corner" style="border-radius:{r}px"></span>{l}
            </label>
          {/each}
        </div>
      </div>
    </section>

    <section class="card sec">
      <h2>Шрифт</h2>
      <div class="fonts" role="radiogroup" aria-label="Шрифт портала">
        {#each Object.entries(FONTS) as [k, f] (k)}
          <label class="font-opt" class:on={draft.font_family === k} style="font-family: {f.stack}">
            <input type="radio" name="font" value={k} bind:group={draft.font_family} />
            <span class="fs-sample">Аа Бб 123</span>
            <span class="fs-name">{f.label}</span>
          </label>
        {/each}
      </div>
      <div class="opt-group">
        <span class="opt-label">Размер основного текста</span>
        <div class="opts">
          {#each Object.entries(FONT_SIZES) as [k, v] (k)}
            <label class="opt" class:on={draft.font_size === k}><input type="radio" name="fsize" value={k} bind:group={draft.font_size} /><span style="font-size:{v.px}px">{v.label}</span></label>
          {/each}
        </div>
      </div>
      <p class="faint small">Все шрифты поддерживают кириллицу. «Системный» не требует загрузки; остальные подгружаются с Google Fonts, поэтому браузеру нужен доступ к fonts.googleapis.com — без него будет показан системный шрифт.</p>
    </section>

    <section class="card sec">
      <h2>Страница входа</h2>
      <label class="field">Заголовок
        <input class="input" bind:value={draft.login_title} placeholder={logoUrl || iconUrl ? 'Не показывать' : previewName} maxlength="120" />
        <span class="faint small">{logoUrl || iconUrl ? 'Оставьте пустым, чтобы под логотипом не было заголовка.' : 'Если пусто — показывается название портала.'}</span>
      </label>
      <label class="field">Подзаголовок <textarea class="input" rows="2" bind:value={draft.login_subtitle} maxlength="400" placeholder="Например: рабочее пространство инженеров"></textarea></label>
      {#if logoUrl}
        <label class="check"><input type="checkbox" bind:checked={draft.login_logo_plate} />Белая подложка под логотипом</label>
        {#if !draft.login_logo_plate}<p class="faint small">Без подложки логотип лежит прямо на фоне — подходит для светлых или контурных логотипов.</p>{/if}
      {/if}
      <div class="asset">
        <div class="asset-head"><b>Фоновое изображение</b><span class="faint small">JPEG, WebP, PNG или SVG · до 3 МБ · лучше горизонтальное фото от 1600 px · вместо фирменного градиента под логотипом</span></div>
        <div class="asset-body">
          <div class="thumb bg">{#if bgUrl}<img src={bgUrl} alt="Фон страницы входа" />{:else}<span class="faint small">нет</span>{/if}</div>
          <div class="row">
            <label class="btn sm">Загрузить<input type="file" accept="image/png,image/jpeg,image/webp,image/svg+xml" hidden onchange={(e) => onAsset('login_bg', e)} /></label>
            {#if bgUrl}<button class="btn ghost sm danger" onclick={() => removeAsset('login_bg')}>Удалить</button>{/if}
          </div>
        </div>
      </div>
      <div class="field">
        <span>Подложка</span>
        <div class="bases" role="radiogroup" aria-label="Подложка под фоном">
          {#each LOGIN_BASES as b (b.key)}
            <button class="base" class:on={draft.login_bg_base === b.key} role="radio" aria-checked={draft.login_bg_base === b.key} onclick={() => setBase(b.key)}>
              <i class="sw-b" style={b.key === 'brand' ? 'background: linear-gradient(155deg, var(--brand), color-mix(in srgb, var(--brand) 70%, black))' : `background: ${'color' in b ? b.color : ''}`}></i>{b.label}
            </button>
          {/each}
          <label class="base" class:on={customBase} title="Свой цвет">
            <input type="color" value={customBase ? draft.login_bg_base : '#e8eef6'} oninput={(e) => setCustomBase(e.currentTarget.value)} aria-label="Свой цвет подложки" />Свой цвет
          </label>
        </div>
        <span class="faint small">Видна под прозрачными частями фонового изображения (PNG или SVG с прозрачностью), а без изображения — заливает всю панель.</span>
      </div>
      <div class="field">
        <span>Тон фона</span>
        <div class="seg tone" role="radiogroup" aria-label="Тон фона">
          <button role="radio" aria-checked={draft.login_bg_tone === 'dark'} class:on={draft.login_bg_tone === 'dark'} onclick={() => (draft.login_bg_tone = 'dark')}>Тёмный</button>
          <button role="radio" aria-checked={draft.login_bg_tone === 'light'} class:on={draft.login_bg_tone === 'light'} onclick={() => (draft.login_bg_tone = 'light')}>Светлый</button>
        </div>
        <span class="faint small">{draft.login_bg_tone === 'light' ? 'Тёмный текст и значки — для светлой подложки и светлых изображений.' : 'Светлый текст — для фирменного цвета, тёмной подложки и фотографий.'}</span>
      </div>
      {#if bgUrl}
        <label class="field range">{draft.login_bg_tone === 'light' ? 'Осветление' : 'Затемнение'} фона: {draft.login_bg_overlay}%
          <input type="range" min="0" max="85" step="5" bind:value={draft.login_bg_overlay} />
          <span class="faint small">{draft.login_bg_tone === 'light' ? 'Белая вуаль делает тёмный текст читаемым на пёстрых изображениях; для узоров на прозрачном фоне обычно хватает 0–20%.' : 'Затемнение делает заголовок читаемым на светлых и пёстрых фото.'}</span>
        </label>
      {/if}
      {#if loginLink}
        <div class="field">
          <span class="muted small">Ссылка на вход для сотрудников (портал подставится автоматически)</span>
          <div class="row">
            <input class="input mono" readonly value={loginLink} onfocus={(e) => e.currentTarget.select()} />
            <button class="btn" onclick={() => navigator.clipboard?.writeText(loginLink)}>Копировать</button>
          </div>
        </div>
      {/if}
    </section>
  </div>

  <aside class="side">
    <div class="sticky">
      <div class="row prev-head">
        <div class="seg">
          <button class:on={previewMode === 'app'} onclick={() => (previewMode = 'app')}>Интерфейс</button>
          <button class:on={previewMode === 'login'} onclick={() => (previewMode = 'login')}>Вход</button>
        </div>
        <div class="spacer"></div>
        <div class="seg" title={draft.color_scheme === 'auto' ? '' : 'Схема зафиксирована настройкой'}>
          <button class:on={previewTheme === 'light'} disabled={draft.color_scheme === 'dark'} onclick={() => (previewTheme = 'light')}>Светлая</button>
          <button class:on={previewTheme === 'dark'} disabled={draft.color_scheme === 'light'} onclick={() => (previewTheme = 'dark')}>Тёмная</button>
        </div>
      </div>

      <div class="preview brand-scope" bind:this={previewEl} aria-label="Предпросмотр" role="img">
        {#if previewMode === 'app'}
          <div class="p-shell">
            <div class="p-side" data-sidebar={draft.sidebar_style}>
              <div class="p-brand">
                {#if logoUrl}<img class="p-logo" class:plate={draft.logo_plate} src={logoUrl} alt="" />{:else}
                  {#if iconUrl}<img src={iconUrl} alt="" width="20" height="20" class="p-icon" />{:else}<span class="p-mark"></span>{/if}
                  <span class="p-name">{previewName}</span>
                {/if}
              </div>
              <span class="p-nav on">Документы</span>
              <span class="p-nav">Доступные мне</span>
              <span class="p-nav">Проекты</span>
              <span class="p-nav">Администрирование</span>
            </div>
            <div class="p-main">
              <div class="p-tabs"><span class="on">Мои документы</span><span>Общие документы</span></div>
              <div class="p-head"><b>Обследование корпуса 12</b><span class="p-btn">Загрузить</span></div>
              <div class="p-card">
                <div class="p-row"><span class="p-ic w"></span>Программа обследования.docx<span class="p-badge">v3</span></div>
                <div class="p-row"><span class="p-ic x"></span>Ведомость дефектов.xlsx<span class="p-link">Скачать</span></div>
                <div class="p-row"><span class="p-ic p"></span>Технический отчёт.pdf<span class="p-link">Скачать</span></div>
              </div>
              <div class="p-bar"><div></div></div>
            </div>
          </div>
        {:else}
          <div class="p-login">
            <div class="p-hero" class:photo={!!bgUrl} class:light={draft.login_bg_tone === 'light'} style={loginHero(draft, bgUrl)}>
              {#if logoUrl}<span class="p-plate" class:bare={!draft.login_logo_plate}><img src={logoUrl} alt="" /></span>{:else if iconUrl}<img src={iconUrl} alt="" width="28" height="28" />{/if}
              {#if draft.login_title.trim() || !(logoUrl || iconUrl)}<b>{draft.login_title.trim() || previewName}</b>{/if}
              {#if draft.login_subtitle.trim()}<span>{draft.login_subtitle}</span>{/if}
            </div>
            <div class="p-form">
              <b>Вход</b>
              <span class="p-input"></span><span class="p-input"></span><span class="p-input"></span>
              <span class="p-btn full">Войти</span>
            </div>
          </div>
        {/if}
      </div>

      <div class="row actions">
        <button class="btn ghost" onclick={resetDefaults}>Стандартное оформление</button>
        <div class="spacer"></div>
        {#if saved && dirty}<button class="btn" onclick={() => saved && take(saved)}>Отменить</button>{/if}
        <button class="btn primary" disabled={!dirty || busy} onclick={save}>Сохранить</button>
      </div>
      {#if dirty}<p class="faint small">Есть несохранённые изменения.</p>{/if}
    </div>
  </aside>
</div>

<style>
  .ok-box, .error-box { margin-bottom: 12px; }
  .layout { display: grid; grid-template-columns: minmax(0, 1fr) minmax(360px, 460px); gap: 16px; align-items: start; }
  .form { display: grid; gap: 12px; }
  .sec { padding: 16px; display: grid; gap: 12px; }
  .sec h2 { margin: 0; }
  .sec p { margin: 0; }

  .assets { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .asset { border: 1px solid var(--border); border-radius: var(--radius); padding: 10px; display: grid; gap: 8px; }
  .asset-head { display: grid; gap: 2px; }
  .asset-body { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .thumb {
    display: grid; place-items: center; border: 1px dashed var(--border-strong); border-radius: var(--radius);
    flex: none; overflow: hidden; padding: 6px; box-sizing: border-box;
    /* Always light, so that dark logos stay visible in dark mode. */
    background: repeating-conic-gradient(#eef0f3 0 25%, #ffffff 0 50%) 0 0 / 12px 12px;
  }
  .thumb.wide { width: 150px; height: 56px; }
  .thumb.square { width: 56px; height: 56px; }
  /* Fill the box and letterbox: percentage max-sizes do not resolve inside a grid
     cell, so an SVG without intrinsic size would otherwise overflow. */
  .thumb img { width: 100%; height: 100%; min-width: 0; min-height: 0; object-fit: contain; display: block; }

  .swatches { display: flex; gap: 8px; flex-wrap: wrap; }
  .swatch { width: 30px; height: 30px; border-radius: 50%; border: 2px solid var(--surface); box-shadow: 0 0 0 1px var(--border-strong); cursor: pointer; }
  .swatch.on { box-shadow: 0 0 0 2px var(--text); }
  .color-row { flex-wrap: wrap; }
  .picker { width: 44px; height: 32px; padding: 0; border: 1px solid var(--border-strong); border-radius: var(--radius); background: none; cursor: pointer; }
  .hex { width: 110px; }
  .warn-note { background: var(--warn-soft); color: var(--warn); padding: 8px 12px; border-radius: var(--radius); font-size: 13px; }

  .opt-group { display: grid; gap: 6px; }
  .opt-label { font-size: 13px; color: var(--text-2); }
  .opts { display: flex; gap: 8px; flex-wrap: wrap; }
  .opt {
    display: inline-flex; align-items: center; gap: 8px; padding: 6px 12px; cursor: pointer;
    border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface);
  }
  .opt.on { border-color: var(--accent); background: var(--accent-soft); color: var(--accent); }
  .opt input { position: absolute; opacity: 0; pointer-events: none; }
  .opt:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: 1px; }
  .mini-side { width: 14px; height: 20px; border-radius: 3px; border: 1px solid var(--border-strong); background: #fff; }
  .mini-side.dark { background: #151b24; }
  .mini-side.accent { background: var(--sw); }
  .mini-corner { width: 18px; height: 14px; border: 2px solid currentColor; }

  /* The column stretches to the full height so the preview can stay in view while scrolling. */
  .layout > .side { align-self: stretch; }
  .seg.tone { justify-self: start; width: auto; }
  .sticky { position: sticky; top: 16px; display: grid; gap: 10px; }
  .prev-head { flex-wrap: wrap; }
  .seg button:disabled { opacity: .4; cursor: default; }
  .actions { flex-wrap: wrap; }
  .actions + p { margin: 0; }

  /* ---- Preview (scaled-down app) ---- */
  .preview {
    border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden;
    background: var(--bg); color: var(--text); font-size: 11px; height: 300px; box-shadow: var(--shadow);
  }
  .p-shell { display: grid; grid-template-columns: 128px 1fr; height: 100%; }
  .p-side { background: var(--surface); color: var(--text); border-right: 1px solid var(--border); padding: 10px 6px; display: grid; align-content: start; gap: 2px; }
  .p-brand { display: flex; align-items: center; gap: 6px; padding: 2px 6px 10px; min-height: 34px; }
  .p-logo { height: 24px; width: auto; max-width: 100%; object-fit: contain; object-position: left center; }
  .p-logo.plate { background: #fff; padding: 3px 5px; border-radius: var(--radius); max-height: 28px; }
  .p-icon { border-radius: 4px; }
  .p-mark { width: 18px; height: 18px; border-radius: 5px; background: var(--accent); flex: none; }
  .p-name { font-weight: 600; line-height: 1.2; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .p-nav { padding: 5px 8px; border-radius: var(--radius); color: var(--text-2); }
  .p-nav.on { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .p-main { padding: 10px 12px; display: grid; gap: 8px; align-content: start; min-width: 0; }
  .p-tabs { display: flex; gap: 10px; border-bottom: 1px solid var(--border); }
  .p-tabs span { padding-bottom: 4px; color: var(--text-2); }
  .p-tabs .on { color: var(--accent); border-bottom: 2px solid var(--accent); }
  .p-head { display: flex; align-items: center; justify-content: space-between; font-size: 12.5px; }
  .p-btn { background: var(--accent); color: var(--on-accent); padding: 4px 9px; border-radius: var(--radius); font-weight: 500; }
  .p-btn.full { text-align: center; }
  .p-card { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-lg); }
  .p-row { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--border); white-space: nowrap; overflow: hidden; }
  .p-row:last-child { border-bottom: 0; }
  .p-ic { width: 12px; height: 12px; border-radius: 2px; flex: none; }
  .p-ic.w { background: var(--word); } .p-ic.x { background: var(--cell); } .p-ic.p { background: var(--pdf); }
  .p-badge { margin-left: auto; background: var(--accent-soft); color: var(--accent); padding: 0 6px; border-radius: 999px; }
  .p-link { margin-left: auto; color: var(--accent); }
  .p-bar { height: 5px; background: var(--surface-2); border-radius: 3px; overflow: hidden; }
  .p-bar div { width: 62%; height: 100%; background: var(--accent); }
  .p-login { display: grid; grid-template-columns: 1fr 1fr; height: 100%; }
  .p-hero {
    background: var(--hero-bg, linear-gradient(160deg, var(--brand), color-mix(in srgb, var(--brand) 70%, black)));
    color: var(--hero-fg, var(--on-brand)); padding: 16px; display: grid; align-content: center; gap: 6px;
  }
  .p-hero b { font-size: 14px; line-height: 1.2; }
  .p-hero span { opacity: .85; }
  .p-plate { background: #fff; border-radius: var(--radius); padding: 5px 8px; justify-self: start; }
  .p-plate img { height: 26px; width: auto; max-width: 110px; display: block; object-fit: contain; }
  .p-plate.bare { background: none; padding: 0; }
  .bases { display: flex; flex-wrap: wrap; gap: 6px; }
  .base { display: inline-flex; align-items: center; gap: 8px; height: 34px; padding: 0 12px 0 8px; border: 1px solid var(--border); border-radius: var(--radius);
          background: var(--surface); color: var(--text); font: inherit; font-size: 13px; cursor: pointer; }
  .base:hover { border-color: var(--border-strong); }
  .base.on { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent) inset; }
  .sw-b { width: 20px; height: 20px; border-radius: 5px; border: 1px solid var(--border); display: inline-block; }
  .base input[type='color'] { width: 22px; height: 22px; padding: 0; border: 1px solid var(--border); border-radius: 5px; background: none; cursor: pointer; }
  .preview { font-family: var(--font); }
  .fonts { display: grid; grid-template-columns: repeat(auto-fill, minmax(130px, 1fr)); gap: 8px; }
  .font-opt { display: grid; gap: 2px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--radius); cursor: pointer; position: relative; }
  .font-opt:hover { border-color: var(--border-strong); }
  .font-opt.on { border-color: var(--accent); background: var(--accent-soft); box-shadow: 0 0 0 1px var(--accent) inset; }
  .font-opt input { position: absolute; opacity: 0; pointer-events: none; }
  .font-opt:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: 2px; }
  .fs-sample { font-size: 18px; font-weight: 600; line-height: 1.2; }
  .fs-name { font-size: 12px; color: var(--text-2); }
  .thumb.bg { width: 180px; height: 100px; padding: 0; }
  .thumb.bg img { object-fit: cover; }
  .range input { width: 100%; max-width: 320px; accent-color: var(--accent); }
  .p-form { padding: 16px; display: grid; align-content: center; gap: 7px; background: var(--bg); }
  .p-input { height: 18px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface); }

  @media (max-width: 1100px) {
    .layout { grid-template-columns: 1fr; }
    .sticky { position: static; }
  }
  @media (max-width: 560px) { .assets { grid-template-columns: 1fr; } }
</style>
