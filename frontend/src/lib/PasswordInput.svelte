<script lang="ts">
  // Password field of the sign-in pages: lock icon, «show password» toggle and
  // a Caps Lock warning (the most common reason for «wrong password»).
  import Lock from '@lucide/svelte/icons/lock';
  import Eye from '@lucide/svelte/icons/eye';
  import EyeOff from '@lucide/svelte/icons/eye-off';
  import { passwordStrength } from '#lib/passwordStrength.ts';

  let {
    value = $bindable(''),
    autocomplete = 'current-password',
    minlength,
    required = true,
    placeholder = '',
    strength = false,
    invalid = false,
    label = undefined
  }: {
    value?: string;
    autocomplete?: 'current-password' | 'new-password';
    minlength?: number;
    required?: boolean;
    placeholder?: string;
    /** Show the strength bar (new passwords). */
    strength?: boolean;
    invalid?: boolean;
    label?: string;
  } = $props();
  const st = $derived(passwordStrength(value, minlength ?? 10));

  let shown = $state(false);
  let caps = $state(false);
  const check = (e: KeyboardEvent) => (caps = e.getModifierState?.('CapsLock') ?? false);
</script>

<span class="with-icon pw">
  <Lock size={17} />
  <input class="input" type={shown ? 'text' : 'password'} bind:value {autocomplete} {minlength} {required} {placeholder}
         aria-invalid={invalid || undefined} aria-label={label}
         onkeydown={check} onkeyup={check} onblur={() => (caps = false)} />
  <button type="button" class="eye" onclick={() => (shown = !shown)} aria-label={shown ? 'Скрыть пароль' : 'Показать пароль'}
          title={shown ? 'Скрыть пароль' : 'Показать пароль'}>
    {#if shown}<EyeOff size={17} />{:else}<Eye size={17} />{/if}
  </button>
</span>
{#if caps}<span class="caps">Включён Caps Lock</span>{/if}
{#if strength && value}
  <span class="meter s{st.score}" role="status" aria-label="Надёжность пароля: {st.label}">
    <span class="bars"><i></i><i></i><i></i><i></i></span><span class="lbl">{st.score ? `Пароль ${st.label}` : st.label}</span>
  </span>
{/if}

<style>
  .pw { position: relative; display: block; }
  .pw .input { padding-right: 40px; }
  .eye { position: absolute; right: 6px; top: 50%; transform: translateY(-50%); border: 0; background: none;
         color: var(--text-3); cursor: pointer; padding: 6px; border-radius: 6px; display: grid; }
  .eye:hover { color: var(--text); background: var(--surface-2); }
  .meter { display: flex; align-items: center; gap: 10px; margin-top: 6px; font-size: 12.5px; color: var(--text-3); }
  .bars { display: grid; grid-template-columns: repeat(4, 1fr); gap: 4px; width: 120px; }
  .bars i { height: 4px; border-radius: 2px; background: var(--border); }
  .s1 .bars i:nth-child(-n+1) { background: var(--danger); }
  .s2 .bars i:nth-child(-n+2) { background: var(--warn); }
  .s3 .bars i:nth-child(-n+3) { background: var(--ok); }
  .s4 .bars i { background: var(--ok); }
  .s1 .lbl { color: var(--danger); }
  .s2 .lbl { color: var(--warn); }
  .s3 .lbl, .s4 .lbl { color: var(--ok); }
  .caps { display: block; margin-top: 4px; font-size: 12.5px; color: var(--warn); }
</style>
