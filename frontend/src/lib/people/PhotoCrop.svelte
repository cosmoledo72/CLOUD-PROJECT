<script lang="ts">
  // Choose a photo and frame it: drag to move, slider to zoom; the square
  // inside the circle is uploaded as a 400×400 JPEG.
  import Modal from '$lib/Modal.svelte';
  import ImageUp from '@lucide/svelte/icons/image-up';

  let { open = $bindable(false), onsave, onremove }: { open?: boolean; onsave: (file: File) => Promise<void>; /** Shown when there is a photo. */ onremove?: () => Promise<void> } = $props();

  const VIEW = 260; // preview size, px
  const OUT = 400; // uploaded size, px
  let img = $state<HTMLImageElement | null>(null);
  let zoom = $state(1);
  let dx = $state(0);
  let dy = $state(0);
  let error = $state('');
  let busy = $state(false);
  let input: HTMLInputElement;

  // Scale at zoom 1: the shorter side fills the frame.
  const base = $derived(img ? VIEW / Math.min(img.naturalWidth, img.naturalHeight) : 1);
  const w = $derived(img ? img.naturalWidth * base * zoom : 0);
  const h = $derived(img ? img.naturalHeight * base * zoom : 0);
  // Keep the frame covered.
  const clampX = (x: number) => Math.min(0, Math.max(VIEW - w, x));
  const clampY = (y: number) => Math.min(0, Math.max(VIEW - h, y));
  const x = $derived(clampX(dx));
  const y = $derived(clampY(dy));

  $effect(() => {
    if (!open) { img = null; error = ''; zoom = 1; }
  });

  function pick(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    (e.currentTarget as HTMLInputElement).value = '';
    if (!f) return;
    if (!/^image\/(png|jpeg|webp)$/.test(f.type)) { error = 'Нужна фотография PNG, JPEG или WEBP'; return; }
    if (f.size > 15 * 1024 * 1024) { error = 'Файл больше 15 МБ'; return; }
    error = '';
    const url = URL.createObjectURL(f);
    const im = new Image();
    im.onload = () => {
      img = im;
      zoom = 1;
      const b = VIEW / Math.min(im.naturalWidth, im.naturalHeight);
      dx = (VIEW - im.naturalWidth * b) / 2;
      dy = (VIEW - im.naturalHeight * b) / 2;
    };
    im.onerror = () => (error = 'Не удалось открыть изображение');
    im.src = url;
  }

  let drag: { px: number; py: number; x: number; y: number } | null = null;
  function down(e: PointerEvent) {
    if (!img) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { px: e.clientX, py: e.clientY, x, y };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    dx = drag.x + e.clientX - drag.px;
    dy = drag.y + e.clientY - drag.py;
  }
  function setZoom(z: number) {
    // Zoom around the centre of the frame.
    const cx = (VIEW / 2 - x) / w, cy = (VIEW / 2 - y) / h;
    zoom = z;
    const nw = img!.naturalWidth * base * z, nh = img!.naturalHeight * base * z;
    dx = VIEW / 2 - cx * nw;
    dy = VIEW / 2 - cy * nh;
  }
  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 20 : 5;
    if (e.key === 'ArrowLeft') dx = x + step;
    else if (e.key === 'ArrowRight') dx = x - step;
    else if (e.key === 'ArrowUp') dy = y + step;
    else if (e.key === 'ArrowDown') dy = y - step;
    else return;
    e.preventDefault();
  }

  async function save() {
    if (!img) return;
    busy = true;
    try {
      const c = document.createElement('canvas');
      c.width = OUT;
      c.height = OUT;
      const k = img.naturalWidth / w; // source px per preview px
      c.getContext('2d')!.drawImage(img, -x * k, -y * k, VIEW * k, VIEW * k, 0, 0, OUT, OUT);
      const blob = await new Promise<Blob | null>((r) => c.toBlob(r, 'image/jpeg', 0.9));
      if (!blob) throw new Error('Не удалось подготовить изображение');
      await onsave(new File([blob], 'photo.jpg', { type: 'image/jpeg' }));
      open = false;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Фото профиля" bind:open width={420}>
  <input bind:this={input} type="file" accept="image/png,image/jpeg,image/webp" hidden onchange={pick} />
  {#if error}<div class="error-box">{error}</div>{/if}
  {#if img}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div class="frame" style="width:{VIEW}px;height:{VIEW}px" onpointerdown={down} onpointermove={move} onpointerup={() => (drag = null)}
      onkeydown={key} tabindex="0" role="application" aria-label="Кадр: перетащите фото или используйте стрелки">
      <img src={img.src} alt="" draggable="false" style="width:{w}px;height:{h}px;transform:translate({x}px,{y}px)" />
      <div class="mask"></div>
    </div>
    <label class="zoom">Масштаб <input type="range" min="1" max="3" step="0.01" value={zoom} oninput={(e) => setZoom(+e.currentTarget.value)} /></label>
    <p class="faint small center">Перетащите фото, чтобы лицо было в круге.</p>
  {:else}
    <button class="drop" onclick={() => input.click()}><ImageUp size={28} /><b>Выбрать фотографию</b><span class="faint small">PNG, JPEG или WEBP</span></button>
  {/if}
  {#snippet footer()}
    {#if img}<button class="btn" onclick={() => input.click()}>Другое фото</button>
    {:else if onremove}<button class="btn danger-text" onclick={async () => { await onremove(); open = false; }}>Удалить фото</button>{/if}
    <div style="flex:1"></div>
    <button class="btn" onclick={() => (open = false)}>Отмена</button>
    <button class="btn primary" disabled={!img || busy} onclick={save}>{busy ? 'Сохранение…' : 'Сохранить'}</button>
  {/snippet}
</Modal>

<style>
  .frame { position: relative; overflow: hidden; margin: 4px auto 12px; border-radius: 12px; background: #111; cursor: grab; touch-action: none; }
  .frame:active { cursor: grabbing; }
  .frame:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .frame img { position: absolute; left: 0; top: 0; max-width: none; user-select: none; pointer-events: none; }
  .mask { position: absolute; inset: 0; border-radius: 50%; box-shadow: 0 0 0 999px rgba(0, 0, 0, .55); pointer-events: none; }
  .zoom { display: flex; gap: 10px; align-items: center; font-size: 13px; color: var(--text-2); }
  .zoom input { flex: 1; }
  .danger-text { color: var(--danger); }
  .center { text-align: center; margin: 8px 0 0; }
  .drop { width: 100%; display: grid; justify-items: center; gap: 6px; padding: 34px 16px; border: 1.5px dashed var(--border-strong, var(--border)); border-radius: 12px;
    background: var(--surface-2); color: var(--text-2); font: inherit; cursor: pointer; }
  .drop:hover { border-color: var(--accent); color: var(--accent); }
</style>
