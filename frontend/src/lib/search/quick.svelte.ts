// Quick create («+ Создать», hotkey C): one menu instance in the layout,
// opened from the sidebar, the mobile bar or the keyboard.
export const quick = $state({ menu: false, x: 0, y: 0 });

export function openQuick(anchor?: HTMLElement | null) {
  const r = anchor?.getBoundingClientRect();
  if (r && r.width) {
    quick.x = Math.min(r.left, window.innerWidth - 270);
    quick.y = r.bottom + 6;
  } else {
    quick.x = Math.max(12, window.innerWidth / 2 - 130);
    quick.y = 80;
  }
  quick.menu = true;
}
