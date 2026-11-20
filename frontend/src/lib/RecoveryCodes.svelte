<script lang="ts">
  // Recovery codes of two-factor authentication (iteration 32): shown once,
  // right after 2FA is turned on or a new set is issued. «Готово» opens only
  // after the person confirms the codes are saved.
  import Modal from '#lib/Modal.svelte';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Copy from '@lucide/svelte/icons/copy';
  import Download from '@lucide/svelte/icons/download';
  import Printer from '@lucide/svelte/icons/printer';

  let {
    open = $bindable(false),
    codes,
    reissued = false,
    portal = '',
    email = ''
  }: { open?: boolean; codes: string[]; reissued?: boolean; portal?: string; email?: string } = $props();

  let saved = $state(false);
  let copied = $state(false);
  $effect(() => {
    if (open) {
      saved = false;
      copied = false;
    }
  });

  const header = $derived(
    `Резервные коды CLOUD-PROJECT${portal ? ` · портал ${portal}` : ''}${email ? ` · ${email}` : ''}\n` +
      `Выпущены ${new Date().toLocaleString('ru-RU')}. Каждый код действует один раз.\n\n`
  );

  async function copy() {
    try {
      await navigator.clipboard.writeText(codes.join('\n'));
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      /* clipboard not available */
    }
  }
  function download() {
    const blob = new Blob([header + codes.join('\n') + '\n'], { type: 'text/plain;charset=utf-8' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `cloud-project-recovery-codes${portal ? '-' + portal : ''}.txt`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }
  function print() {
    const w = window.open('', '_blank', 'width=480,height=600');
    if (!w) return;
    const esc = (s: string) => s.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]!);
    w.document.write(
      `<pre style="font: 15px/1.7 monospace; padding: 24px">${esc(header)}${codes.map(esc).join('\n')}</pre>`
    );
    w.document.close();
    w.focus();
    w.print();
  }
</script>

<Modal bind:open title={reissued ? 'Новые резервные коды' : 'Резервные коды'} width={480}>
  <div class="head">
    {#if reissued}
      <i class="chip-icon accent"><KeyRound size={18} /></i><b>Выпущены новые резервные коды</b>
    {:else}
      <i class="chip-icon ok"><ShieldCheck size={18} /></i><b>Двухфакторная защита включена</b>
    {/if}
  </div>
  <p class="muted small">
    Сохраните резервные коды. Каждый код подходит один раз — вместо кода из приложения, если телефон потерян.
    Коды показываются только сейчас.{#if reissued} Прежние коды больше не действуют.{/if}
  </p>
  <div class="codes">
    {#each codes as c (c)}<span class="rc-code">{c}</span>{/each}
  </div>
  <div class="acts">
    <button class="btn sm" onclick={copy}><Copy size={15} />{copied ? 'Скопировано' : 'Копировать'}</button>
    <button class="btn sm" onclick={download}><Download size={15} />Скачать .txt</button>
    <button class="btn sm" onclick={print}><Printer size={15} />Печать</button>
  </div>
  <label class="check"><input type="checkbox" bind:checked={saved} />Я сохранил коды в надёжном месте</label>
  {#snippet footer()}
    <button class="btn primary" disabled={!saved} onclick={() => (open = false)}>Готово</button>
  {/snippet}
</Modal>

<style>
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; font-size: 15px; }
  p { margin: 0 0 14px; line-height: 1.5; }
  .codes {
    display: grid; grid-template-columns: 1fr 1fr; gap: 8px 18px; padding: 14px 18px;
    background: var(--surface-2); border: 1px solid var(--border); border-radius: 8px;
    font-family: var(--mono); font-size: 15px; letter-spacing: 0.06em;
  }
  .acts { display: flex; flex-wrap: wrap; gap: 8px; margin: 14px 0 12px; }
  .check { display: flex; align-items: center; gap: 8px; font-size: 13.5px; }
</style>
