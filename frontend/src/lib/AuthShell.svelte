<script lang="ts">
  // Two-pane layout of the sign-in pages: portal branding on the left, the form on the right.
  import type { Snippet } from 'svelte';
  import { branding, loginHero, PRODUCT_BRAND, PRODUCT_LOGO_DARK, type Branding } from '#lib/branding.svelte.ts';
  import type { Component } from 'svelte';
  import FileText from '@lucide/svelte/icons/file-text';
  import FolderKanban from '@lucide/svelte/icons/folder-kanban';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import Wallet from '@lucide/svelte/icons/wallet';

  let { brand = null, children, below }: { brand?: Branding | null; children: Snippet; below?: Snippet } = $props();

  // No portal (first start, «Создать портал», unknown address): the product's own look.
  const product = $derived(!brand);
  const b = $derived(brand ?? PRODUCT_BRAND);
  $effect(() => {
    if (!brand && !branding.current) branding.reset();
  });

  // Dark colour scheme (portal setting or the system's): a light-toned panel
  // would glare next to the dark form, so it turns dark with the picture dimmed.
  let systemDark = $state(false);
  $effect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    systemDark = mq.matches;
    const on = (e: MediaQueryListEvent) => (systemDark = e.matches);
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  });
  const dark = $derived(b.color_scheme === 'dark' || (b.color_scheme === 'auto' && systemDark));
  const darkened = $derived(dark && b.login_bg_tone === 'light');
  const heroStyle = $derived(
    darkened
      ? loginHero({ login_bg_tone: 'dark', login_bg_base: '#111b27', login_bg_overlay: 40 }, b.login_bg_url)
      : loginHero(b, b.login_bg_url)
  );
  const logoSrc = $derived(product && dark ? PRODUCT_LOGO_DARK : b.logo_url);
  // A dark logo on the darkened panel gets a white plate.
  const plate = $derived(darkened && !product ? true : b.login_logo_plate !== false);

  // An empty title hides the heading; the portal name is used only when there is
  // no logo or icon either, so the page never ends up without any identity.
  const title = $derived(b.login_title.trim() || (b.logo_url || b.icon_url ? '' : b.display_name));
  const subtitle = $derived(b.login_subtitle);
  // What the portal is for, under the title (not over a custom photo).
  const features: [Component, string][] = [
    [FolderKanban, 'Проекты, задачи и график работ'],
    [FileText, 'Совместное редактирование документов'],
    [Wallet, 'Договоры, счета и оплаты']
  ];
</script>

<div class="wrap">
  <section class="hero" class:photo={!!b.login_bg_url} class:light={b.login_bg_tone === 'light' && !darkened}
           aria-label="Портал" style={heroStyle}>
    <div class="hero-inner">
      {#if logoSrc}
        <div class="logo-plate" class:bare={!plate}><img src={logoSrc} alt={b.display_name} /></div>
      {:else if b.icon_url}
        <img class="hero-icon" src={b.icon_url} alt="" width="56" height="56" />
      {:else}
        <svg viewBox="0 0 32 32" width="52" height="52" aria-hidden="true"><rect width="32" height="32" rx="7" fill="currentColor" opacity=".16"/><path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" transform="translate(4 4.5)" fill="none" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" stroke="currentColor"/></svg>
      {/if}
      <div class="text">
        {#if title}<h1>{title}</h1>{/if}
        {#if subtitle}<p>{subtitle}</p>{/if}
        {#if !b.login_bg_url}
          <ul class="feats">
            {#each features as [Icon, text] (text)}<li><span class="fi"><Icon size={18} /></span>{text}</li>{/each}
          </ul>
        {/if}
      </div>
    </div>
    {#if !product}<div class="powered">Работает на CLOUD-PROJECT</div>{/if}
  </section>

  <section class="form-side">
    <div class="col">
      <div class="panel">
        {@render children()}
      </div>
      {#if below}<div class="below">{@render below()}</div>{/if}
      <p class="secure"><span class="si"><ShieldCheck size={14} /></span>Защищённое соединение · данные хранятся на сервере компании</p>
    </div>
  </section>
</div>

<style>
  .wrap { min-height: 100vh; display: grid; grid-template-columns: minmax(340px, 5fr) 6fr; background: var(--surface); }
  .hero {
    position: relative; overflow: hidden; display: flex; padding: 44px 52px 64px;
    background: var(--hero-bg, linear-gradient(155deg, color-mix(in srgb, var(--brand) 92%, white), color-mix(in srgb, var(--brand) 72%, black)));
    color: var(--hero-fg, var(--on-brand));
  }
  /* Soft geometric shapes over the brand colour. */
  .hero::before, .hero::after {
    content: ''; position: absolute; border-radius: 50%; pointer-events: none;
    border: 1px solid color-mix(in srgb, currentColor 18%, transparent);
  }
  .hero::before { width: 560px; height: 560px; right: -220px; top: -200px; background: color-mix(in srgb, currentColor 6%, transparent); }
  .hero::after { width: 340px; height: 340px; left: -140px; bottom: -150px; background: color-mix(in srgb, currentColor 5%, transparent); }
  /* An uploaded image (over the chosen base colour) replaces the decorative circles. */
  .hero.photo::before, .hero.photo::after { display: none; }
  .hero.photo:not(.light) h1, .hero.photo:not(.light) p { text-shadow: 0 1px 12px rgb(0 0 0 / 35%); }
  /* Light tone: brand-coloured circles, quieter. */
  .hero.light::before, .hero.light::after { border-color: color-mix(in srgb, var(--brand) 14%, transparent); }
  .hero.light::before { background: color-mix(in srgb, var(--brand) 5%, transparent); }
  .hero.light::after { background: color-mix(in srgb, var(--brand) 4%, transparent); }
  .hero.light .powered { opacity: .55; }
  .hero-inner { display: flex; flex-direction: column; gap: 18px; max-width: 460px; position: relative; z-index: 1; width: 100%; }
  .text { margin-top: auto; display: grid; gap: 14px; }
  .hero h1 { font-size: 34px; line-height: 1.15; letter-spacing: -.02em; font-weight: 700; }
  .hero p { margin: 0; font-size: 16px; opacity: .86; line-height: 1.55; }
  .feats { list-style: none; margin: 14px 0 0; padding: 0; display: grid; gap: 12px; }
  .feats li { display: flex; align-items: center; gap: 12px; font-size: 14.5px; opacity: .95; }
  .fi { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; flex: none;
        background: color-mix(in srgb, currentColor 10%, transparent); border: 1px solid color-mix(in srgb, currentColor 16%, transparent); }
  .hero.light .fi { background: rgb(255 255 255 / 70%); color: var(--brand); }
  .logo-plate { background: #fff; border-radius: var(--radius-lg); padding: 12px 16px; align-self: flex-start; box-shadow: 0 6px 20px rgb(0 0 0 / 18%); }
  /* Explicit height: an SVG with only a viewBox has no intrinsic size and would collapse. */
  .logo-plate img { display: block; height: 56px; width: auto; max-width: 260px; object-fit: contain; }
  .logo-plate.bare { background: none; padding: 0; box-shadow: none; border-radius: 0; }
  .hero-icon { border-radius: 12px; }
  .powered { position: absolute; left: 52px; bottom: 24px; font-size: 12px; opacity: .6; z-index: 1; }
  /* Form side: a quiet dotted base with the form on a card. */
  .form-side {
    display: grid; place-items: center; padding: 40px 24px;
    background: radial-gradient(circle, color-mix(in srgb, var(--text-3) 26%, transparent) 1px, transparent 1.3px) 0 0 / 22px 22px, var(--bg);
  }
  .col { width: min(470px, 100%); display: grid; gap: 0; }
  .panel {
    background: var(--surface); border-radius: 16px; padding: 36px 35px 32px;
    border: 1px solid var(--border); border-top: 4px solid var(--brand);
    box-shadow: 0 1px 2px rgb(16 24 40 / 5%), 0 12px 40px rgb(16 24 40 / 9%);
  }
  .below { display: flex; justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap;
           padding: 16px 6px 0; font-size: 13.5px; color: var(--text-2); }
  .below :global(a), .below :global(button) { color: var(--accent); font-weight: 500; }
  .secure { margin: 30px 0 0; font-size: 12px; line-height: 1.6; color: var(--text-3); text-align: center; }
  .si { display: inline-flex; vertical-align: -2px; margin-right: 6px; }
  .panel :global(form), .panel :global(.stack) { display: grid; gap: 14px; }
  .panel :global(h2) { font-size: 26px; font-weight: 650; letter-spacing: -.015em; margin: 0 0 2px; }
  .panel :global(.input) { height: 42px; }
  .panel :global(.btn.primary) { height: 44px; font-size: 15px; }
  .panel :global(.btn.full) { width: 100%; justify-content: center; }
  .panel :global(p) { margin: 0; }
  @media (max-width: 860px) {
    .wrap { grid-template-columns: 1fr; grid-template-rows: auto 1fr; }
    .hero { padding: 24px 20px 26px; }
    .hero h1 { font-size: 22px; }
    .text { margin-top: 10px; }
    .feats, .powered { display: none; }
    .form-side { place-items: start center; padding: 20px 14px 28px; }
    .panel { padding: 26px 20px 24px; box-shadow: 0 1px 2px rgb(16 24 40 / 5%); }
    .secure { margin-top: 20px; }
  }
</style>
