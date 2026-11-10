<script lang="ts">
  // Two-pane layout of the sign-in pages: portal branding on the left, the form on the right.
  import type { Snippet } from 'svelte';
  import type { Branding } from '$lib/branding.svelte';

  let { brand = null, children }: { brand?: Branding | null; children: Snippet } = $props();

  // An empty title hides the heading; the portal name is used only when there is
  // no logo or icon either, so the page never ends up without any identity.
  const title = $derived(
    !brand ? 'WorkHub' : brand.login_title.trim() || (brand.logo_url || brand.icon_url ? '' : brand.display_name)
  );
  const subtitle = $derived(
    brand ? brand.login_subtitle : 'Документы, совместное редактирование и проекты в одном рабочем пространстве'
  );
</script>

<div class="wrap">
  <section class="hero" class:photo={!!brand?.login_bg_url} aria-label="Портал"
           style={brand?.login_bg_url ? `--bg-img: url('${brand.login_bg_url}'); --bg-dim: ${brand.login_bg_overlay ?? 45}%` : ''}>
    <div class="hero-inner">
      {#if brand?.logo_url}
        <div class="logo-plate" class:bare={brand.login_logo_plate === false}><img src={brand.logo_url} alt={brand.display_name} /></div>
      {:else if brand?.icon_url}
        <img class="hero-icon" src={brand.icon_url} alt="" width="56" height="56" />
      {:else}
        <svg viewBox="0 0 32 32" width="52" height="52" aria-hidden="true"><rect width="32" height="32" rx="7" fill="var(--on-brand)" opacity=".16"/><path d="M8 10h4l2 9 2-9h2l2 9 2-9h4l-4 13h-3l-2-8-2 8h-3z" fill="var(--on-brand)"/></svg>
      {/if}
      {#if title}<h1>{title}</h1>{/if}
      {#if subtitle}<p>{subtitle}</p>{/if}
    </div>
    {#if brand}<div class="powered">Работает на WorkHub</div>{/if}
  </section>

  <section class="form-side">
    <div class="panel">
      {@render children()}
    </div>
  </section>
</div>

<style>
  .wrap { min-height: 100vh; display: grid; grid-template-columns: minmax(320px, 5fr) 6fr; }
  .hero {
    position: relative; display: flex; align-items: center; padding: 48px;
    background:
      radial-gradient(120% 90% at 0% 0%, color-mix(in srgb, var(--brand) 70%, white) 0%, transparent 60%),
      linear-gradient(160deg, var(--brand), color-mix(in srgb, var(--brand) 70%, black));
    color: var(--on-brand);
  }
  /* Uploaded photo instead of the brand gradient, darkened for readable text. */
  .hero.photo {
    background: linear-gradient(rgb(0 0 0 / var(--bg-dim)), rgb(0 0 0 / var(--bg-dim))), var(--bg-img) center / cover no-repeat;
    color: #fff;
  }
  .hero.photo h1, .hero.photo p { text-shadow: 0 1px 12px rgb(0 0 0 / 35%); }
  .hero-inner { display: grid; gap: 14px; max-width: 420px; position: relative; }
  .hero h1 { font-size: 28px; line-height: 1.2; }
  .hero p { margin: 0; font-size: 15px; opacity: .88; line-height: 1.5; }
  .logo-plate { background: #fff; border-radius: var(--radius-lg); padding: 14px 18px; justify-self: start; box-shadow: 0 6px 20px rgb(0 0 0 / 18%); }
  /* Explicit height: an SVG with only a viewBox has no intrinsic size and would collapse. */
  .logo-plate img { display: block; height: 64px; width: auto; max-width: 260px; object-fit: contain; }
  .logo-plate.bare { background: none; padding: 0; box-shadow: none; border-radius: 0; }
  .hero-icon { border-radius: 12px; }
  .powered { position: absolute; left: 48px; bottom: 24px; font-size: 12px; opacity: .7; }
  .form-side { display: grid; place-items: center; padding: 24px 16px; }
  .panel { width: min(380px, 100%); }
  .hero p { margin: 0; }
  .panel :global(form) { display: grid; gap: 12px; }
  .panel :global(.full) { width: 100%; justify-content: center; }
  .panel :global(p) { margin: 0; }
  @media (max-width: 820px) {
    .wrap { grid-template-columns: 1fr; grid-template-rows: auto 1fr; }
    .hero { padding: 28px 20px; }
    .hero h1 { font-size: 22px; }
    .powered { display: none; }
  }
</style>
