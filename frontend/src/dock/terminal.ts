import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { cssToken as token } from './dom';

export interface DockTermOptions { agent: boolean; scrollback?: number | undefined; leaves(event: KeyboardEvent): boolean }

export interface DockTerm {
  cols(): number;
  rows(): number;
  open(host: HTMLElement): void;
  fit(): boolean;
  focus(): void;
  write(data: string | Uint8Array): void;
  reset(): void;
  retheme(): void;
  onInput(handler: (data: string) => void): void;
  dispose(): void;
}

function number(name: string, fallback: number): number {
  const n = parseFloat(token(name, String(fallback)));
  return n > 0 ? n : fallback;
}

function colour(probe: HTMLElement | null, name: string, fallback: string): string {
  const raw = token(name, '');
  if (!raw) return fallback;
  if (!probe || /^#[0-9a-f]{3,8}$/i.test(raw)) return raw;
  probe.style.color = raw;
  const seen = probe.style.color ? getComputedStyle(probe).color : '';
  probe.style.removeProperty('color');
  return /^rgba?\(/.test(seen) ? seen : raw;
}

function theme(probe: HTMLElement | null): { background: string; foreground: string; cursor: string; cursorAccent: string; selectionBackground: string } {
  const background = colour(probe, '--dock-term-bg', '#111111');
  return {
    background,
    foreground: colour(probe, '--dock-term-fg', '#f7f7f7'),
    cursor: colour(probe, '--dock-term-cursor', '#f97316'),
    cursorAccent: background,
    selectionBackground: colour(probe, '--dock-term-selection', '#f9731633'),
  };
}

export function createTerm(o: DockTermOptions): DockTerm {
  const term = new Terminal({
    fontFamily: token('--dock-font-mono', 'monospace'), fontSize: number('--dock-term-font-size', 13), lineHeight: number('--dock-term-line-height', 1.25),
    cursorBlink: !o.agent, disableStdin: o.agent, scrollback: o.scrollback ?? 5000, minimumContrastRatio: number('--dock-term-contrast', 1),
    theme: theme(null), overviewRuler: { width: 0 },
  });
  const fit = new FitAddon();
  let probe: HTMLElement | null = null;
  let look = '';
  term.loadAddon(fit);
  term.attachCustomKeyEventHandler((e) => !o.leaves(e));
  const retheme = (): void => {
    const now = theme(probe);
    const size = number('--dock-term-font-size', 13);
    const next = JSON.stringify([now, size]);
    if (next === look) return;
    look = next;
    term.options.theme = now;
    if (term.options.fontSize !== size) term.options.fontSize = size;
  };
  return {
    cols: () => term.cols,
    rows: () => term.rows,
    open: (host) => {
      probe = host;
      term.open(host);
      retheme();
      if (o.agent) term.write('\x1b[?25l');
    },
    fit: () => {
      const before = `${term.cols}x${term.rows}`;
      fit.fit();
      return `${term.cols}x${term.rows}` !== before;
    },
    focus: () => term.focus(),
    write: (data) => term.write(data),
    reset: () => term.reset(),
    retheme,
    onInput: (handler) => { term.onData(handler); },
    dispose: () => term.dispose(),
  };
}
