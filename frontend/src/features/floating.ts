export interface Floating { place(): void }

export function floating(root: HTMLElement, head: HTMLElement, key: string, fallback: () => { x: number; y: number }): Floating {
  let pos: { x: number; y: number } | null = null;
  const move = (x: number, y: number) => {
    const w = root.offsetWidth || 400;
    const cx = Math.round(Math.max(80 - w, Math.min(window.innerWidth - 80, x)));
    const cy = Math.round(Math.max(8, Math.min(window.innerHeight - 48, y)));
    pos = { x: cx, y: cy };
    root.style.left = `${cx}px`;
    root.style.top = `${cy}px`;
  };
  let start: { px: number; py: number; x: number; y: number } | null = null;
  head.addEventListener('pointerdown', (e) => {
    if ((e.target as HTMLElement).closest('button') || e.button !== 0) return;
    head.setPointerCapture(e.pointerId);
    start = { px: e.clientX, py: e.clientY, x: pos?.x ?? 0, y: pos?.y ?? 0 };
    root.classList.add('is-moving');
  });
  head.addEventListener('pointermove', (e) => { if (start) move(start.x + e.clientX - start.px, start.y + e.clientY - start.py); });
  const end = () => {
    if (!start) return;
    start = null;
    root.classList.remove('is-moving');
    try { localStorage.setItem(key, JSON.stringify(pos)); } catch { }
  };
  head.addEventListener('pointerup', end);
  head.addEventListener('pointercancel', end);
  return {
    place() {
      if (!pos) {
        try { const s = JSON.parse(localStorage.getItem(key) ?? 'null'); if (s && typeof s.x === 'number' && typeof s.y === 'number') pos = s; } catch { pos = null; }
      }
      const p = pos ?? fallback();
      move(p.x, p.y);
    },
  };
}
