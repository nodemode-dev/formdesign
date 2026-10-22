// stylized-nodemode · themes.ts · v1.0.0
// The 14 NodeMode appearance profiles, verbatim from
// frontend/src/components/ui/settings/index.ts (v7.1.0, THEMES), plus the
// runtime applier the app uses (applySettings, reduced to the theme-visible
// parts). Keep the preset objects byte-identical to the app so a companion
// site and NodeMode render the same surface colors for the same profile id.
//
// Boot order that guarantees zero theme flash:
//   1. theme-boot.js runs inline in the document head and applies the
//      persisted profile + accent to documentElement BEFORE body paints.
//   2. this module re-applies the same values once the bundle loads (idempotent)
//      and owns every later change from a theme picker.
// Never call applyTheme() from a component's first render; that is a second
// paint and the user sees both.

export interface ThemeProfile { label: string; vars: Record<string, string>; light?: boolean }

export const THEMES: Record<string, ThemeProfile> = {
  // The target look (Phase 21, UI_SPEC.md): surfaces, text and the pasteboard measured from the owner's screenshots;
  // --grid-color is the pasteboard's dot grid.
  'studio': { label: 'Studio Dark', vars: { '--bg-primary': '#111111', '--bg-secondary': '#17151b', '--bg-tertiary': '#222127', '--border-color': '#2a2a2a', '--border-light': '#34333a', '--text-primary': '#f7f7f7', '--text-secondary': '#a4a4a4', '--text-muted': '#717072', '--canvas-bg': '#1c1b1d', '--grid-color': '#5e5e60', '--bg-node': '#191919' } },
  'nodemode-dark': { label: 'NodeMode Dark', vars: { '--bg-primary': '#0a0a0a', '--bg-secondary': '#121218', '--bg-tertiary': '#1a1a24', '--border-color': '#2a2a35', '--border-light': '#3a3a48', '--text-primary': '#e8e8ee', '--text-secondary': '#a8a8b8', '--text-muted': '#6a6a7a', '--canvas-bg': '#0a0a0a', '--grid-color': 'rgba(255,255,255,0.04)', '--node-shadow': '0 6px 18px rgba(0,0,0,0.45)' } },
  'light': { label: 'Light', light: true, vars: { '--bg-primary': '#ecebe6', '--bg-secondary': '#f7f6f1', '--bg-tertiary': '#e2e0d8', '--bg-node': '#f4f3ed', '--border-color': '#d1cdc1', '--border-light': '#beb9aa', '--text-primary': '#26241d', '--text-secondary': '#4c4a40', '--text-muted': '#8b887a', '--canvas-bg': '#e7e5dd', '--grid-color': 'rgba(70,60,30,0.10)', '--node-shadow': '0 5px 14px rgba(96,90,70,0.15)' } },
  'github-dark': { label: 'GitHub Dark', vars: { '--bg-primary': '#0d1117', '--bg-secondary': '#161b22', '--bg-tertiary': '#21262d', '--border-color': '#30363d', '--border-light': '#3d444d', '--text-primary': '#e6edf3', '--text-secondary': '#9198a1', '--text-muted': '#656c76', '--canvas-bg': '#0d1117', '--grid-color': 'rgba(230,237,243,0.05)', '--node-shadow': '0 6px 18px rgba(0,0,0,0.5)' } },
  'midnight': { label: 'Midnight Blue', vars: { '--bg-primary': '#0b1020', '--bg-secondary': '#101830', '--bg-tertiary': '#182242', '--border-color': '#25325c', '--border-light': '#324274', '--text-primary': '#e4e9f7', '--text-secondary': '#9fabd0', '--text-muted': '#63709a', '--canvas-bg': '#0b1020', '--grid-color': 'rgba(160,180,255,0.06)', '--node-shadow': '0 8px 22px rgba(0,0,20,0.55)' } },
  'forest': { label: 'Forest', vars: { '--bg-primary': '#0c130e', '--bg-secondary': '#121c15', '--bg-tertiary': '#1a281e', '--border-color': '#2a4030', '--border-light': '#375441', '--text-primary': '#e4efe7', '--text-secondary': '#a4bdac', '--text-muted': '#6a8674', '--canvas-bg': '#0c130e', '--grid-color': 'rgba(180,230,190,0.05)', '--node-shadow': '0 6px 18px rgba(0,10,0,0.5)' } },
  'ember': { label: 'Ember', vars: { '--bg-primary': '#140d0a', '--bg-secondary': '#1d1310', '--bg-tertiary': '#2a1c16', '--border-color': '#452e22', '--border-light': '#5a3d2e', '--text-primary': '#f2e8e2', '--text-secondary': '#c7ab9c', '--text-muted': '#8f7263', '--canvas-bg': '#140d0a', '--grid-color': 'rgba(255,200,160,0.05)', '--node-shadow': '0 6px 18px rgba(20,5,0,0.55)' } },
  'violet': { label: 'Violet Haze', vars: { '--bg-primary': '#100b1c', '--bg-secondary': '#171129', '--bg-tertiary': '#221a3c', '--border-color': '#382c5e', '--border-light': '#493b78', '--text-primary': '#ece6f9', '--text-secondary': '#b3a6d8', '--text-muted': '#7a6da3', '--canvas-bg': '#100b1c', '--grid-color': 'rgba(210,190,255,0.06)', '--node-shadow': '0 8px 22px rgba(10,0,30,0.55)' } },
  'slate': { label: 'Slate', vars: { '--bg-primary': '#101418', '--bg-secondary': '#171d23', '--bg-tertiary': '#212a32', '--border-color': '#32404c', '--border-light': '#415260', '--text-primary': '#e7edf2', '--text-secondary': '#a3b2bf', '--text-muted': '#6b7a87', '--canvas-bg': '#101418', '--grid-color': 'rgba(200,220,240,0.05)', '--node-shadow': '0 6px 18px rgba(0,5,10,0.5)' } },
  'mono': { label: 'Mono Ink', vars: { '--bg-primary': '#0e0e0e', '--bg-secondary': '#161616', '--bg-tertiary': '#202020', '--border-color': '#333333', '--border-light': '#454545', '--text-primary': '#f0f0f0', '--text-secondary': '#ababab', '--text-muted': '#6e6e6e', '--canvas-bg': '#0e0e0e', '--grid-color': 'rgba(255,255,255,0.05)', '--node-shadow': '0 6px 18px rgba(0,0,0,0.5)' } },
  'sand': { label: 'Sand (light)', light: true, vars: { '--bg-primary': '#efe8da', '--bg-secondary': '#f8f3e9', '--bg-tertiary': '#e6dcc8', '--bg-node': '#f5efe2', '--border-color': '#d4c7ab', '--border-light': '#c2b28e', '--text-primary': '#2a2418', '--text-secondary': '#59503c', '--text-muted': '#8d8168', '--canvas-bg': '#eae2d1', '--grid-color': 'rgba(80,60,10,0.10)', '--node-shadow': '0 5px 14px rgba(100,80,30,0.16)' } },
  'ocean': { label: 'Deep Ocean', vars: { '--bg-primary': '#06131c', '--bg-secondary': '#0a1c29', '--bg-tertiary': '#10293c', '--border-color': '#1d425e', '--border-light': '#285578', '--text-primary': '#e2eef7', '--text-secondary': '#96b6cc', '--text-muted': '#5b7d94', '--canvas-bg': '#06131c', '--grid-color': 'rgba(150,210,255,0.06)', '--node-shadow': '0 8px 22px rgba(0,10,20,0.6)' } },
  'rose': { label: 'Rose Noir', vars: { '--bg-primary': '#170b10', '--bg-secondary': '#211117', '--bg-tertiary': '#301a22', '--border-color': '#4d2b38', '--border-light': '#653a49', '--text-primary': '#f5e7ec', '--text-secondary': '#cfa5b3', '--text-muted': '#966b7b', '--canvas-bg': '#170b10', '--grid-color': 'rgba(255,190,215,0.05)', '--node-shadow': '0 6px 18px rgba(25,0,10,0.55)' } },
  'terminal': { label: 'Terminal Green', vars: { '--bg-primary': '#050d05', '--bg-secondary': '#0a140a', '--bg-tertiary': '#102010', '--border-color': '#1f3d1f', '--border-light': '#2b522b', '--text-primary': '#d8f5d8', '--text-secondary': '#8fce8f', '--text-muted': '#568a56', '--canvas-bg': '#050d05', '--grid-color': 'rgba(120,255,120,0.06)', '--node-shadow': '0 6px 18px rgba(0,10,0,0.6)' } },
  'contrast': { label: 'High Contrast', vars: { '--bg-primary': '#000000', '--bg-secondary': '#0a0a0a', '--bg-tertiary': '#161616', '--border-color': '#555555', '--border-light': '#777777', '--text-primary': '#ffffff', '--text-secondary': '#d8d8d8', '--text-muted': '#9a9a9a', '--canvas-bg': '#000000', '--grid-color': 'rgba(255,255,255,0.10)', '--node-shadow': '0 0 0 1px #666' } },
  'nineties': { label: '90s', light: true, vars: { '--bg-primary': '#c0c0c0', '--bg-secondary': '#d4d0c8', '--bg-tertiary': '#b4b0a8', '--bg-node': '#d4d0c8', '--border-color': '#808080', '--border-light': '#ffffff', '--text-primary': '#000000', '--text-secondary': '#202020', '--text-muted': '#585858', '--canvas-bg': '#008080', '--grid-color': 'rgba(0,0,0,0.12)', '--node-shadow': '2px 2px 0 rgba(0,0,0,0.45)' } },
  'duck': { label: 'Duck', light: true, vars: { '--bg-primary': '#e9e4f7', '--bg-secondary': '#f4f1fc', '--bg-tertiary': '#d9d2ef', '--bg-node': '#f4f1fc', '--border-color': '#c7bde6', '--border-light': '#b3a6dc', '--text-primary': '#2b2540', '--text-secondary': '#4f4770', '--text-muted': '#7a7299', '--canvas-bg': '#e2dcf2', '--grid-color': 'rgba(90,70,150,0.10)', '--node-shadow': '0 5px 14px rgba(70,50,130,0.16)' } },
  'pro-dark': { label: 'Pro Dark', vars: { '--bg-primary': '#1f1f1f', '--bg-secondary': '#2b2b2b', '--bg-tertiary': '#363636', '--border-color': '#4a4a4a', '--border-light': '#818181', '--text-primary': '#ececec', '--text-secondary': '#bdbdbd', '--text-muted': '#a0a0a0', '--canvas-bg': '#181818', '--grid-color': 'rgba(255,255,255,0.05)' } },
  'pro-medium-grey': { label: 'Pro Medium Gray', vars: { '--bg-primary': '#3a3a3a', '--bg-secondary': '#454545', '--bg-tertiary': '#505050', '--border-color': '#666666', '--border-light': '#a0a0a0', '--text-primary': '#f4f4f4', '--text-secondary': '#d6d6d6', '--text-muted': '#c3c3c3', '--canvas-bg': '#2e2e2e', '--grid-color': 'rgba(255,255,255,0.06)' } },
  'pro-light-grey': { label: 'Pro Light Gray', light: true, vars: { '--bg-primary': '#b9b9b9', '--bg-secondary': '#c9c9c9', '--bg-tertiary': '#aeaeae', '--border-color': '#8c8c8c', '--border-light': '#5a5a5a', '--text-primary': '#121212', '--text-secondary': '#2a2a2a', '--text-muted': '#414141', '--canvas-bg': '#a4a4a4', '--grid-color': 'rgba(0,0,0,0.10)' } },
  'pro-light': { label: 'Pro Light', light: true, vars: { '--bg-primary': '#e3e3e3', '--bg-secondary': '#f2f2f2', '--bg-tertiary': '#d6d6d6', '--border-color': '#b5b5b5', '--border-light': '#767676', '--text-primary': '#1a1a1a', '--text-secondary': '#383838', '--text-muted': '#545454', '--canvas-bg': '#d0d0d0', '--grid-color': 'rgba(0,0,0,0.08)' } },
  'classic': { label: 'Classic', light: true, vars: { '--bg-primary': '#d1d4d9', '--bg-secondary': '#e1e4e8', '--bg-tertiary': '#c2c6cc', '--border-color': '#9ea4ad', '--border-light': '#666c76', '--text-primary': '#14171c', '--text-secondary': '#2c323a', '--text-muted': '#4a515b', '--canvas-bg': '#c0c4ca', '--grid-color': 'rgba(0,0,0,0.09)' } },
  'solarized-dark': { label: 'Solarized Dark', vars: { '--bg-primary': '#002b36', '--bg-secondary': '#073642', '--bg-tertiary': '#0d4452', '--border-color': '#1b5563', '--border-light': '#768e97', '--text-primary': '#fdf6e3', '--text-secondary': '#bcc6c6', '--text-muted': '#9bb0b3', '--canvas-bg': '#00212b', '--grid-color': 'rgba(147,161,161,0.08)' } },
  'adwaita-light': { label: 'Adwaita Light', light: true, vars: { '--bg-primary': '#ebebeb', '--bg-secondary': '#fafafa', '--bg-tertiary': '#dedde0', '--border-color': '#c0bfbc', '--border-light': '#77767b', '--text-primary': '#241f31', '--text-secondary': '#3d3846', '--text-muted': '#5e5c64', '--canvas-bg': '#f6f5f4', '--grid-color': 'rgba(0,0,0,0.07)' } },
  'adwaita-dark': { label: 'Adwaita Dark', vars: { '--bg-primary': '#242424', '--bg-secondary': '#303030', '--bg-tertiary': '#3a3a3a', '--border-color': '#4a4a4a', '--border-light': '#858585', '--text-primary': '#ffffff', '--text-secondary': '#deddda', '--text-muted': '#b4b3b0', '--canvas-bg': '#1e1e1e', '--grid-color': 'rgba(255,255,255,0.05)' } },
  'breeze-dark': { label: 'Breeze Dark', vars: { '--bg-primary': '#232629', '--bg-secondary': '#31363b', '--bg-tertiary': '#3b4045', '--border-color': '#4d5359', '--border-light': '#828c94', '--text-primary': '#eff0f1', '--text-secondary': '#c5cacd', '--text-muted': '#a4adb3', '--canvas-bg': '#1b1e20', '--grid-color': 'rgba(239,240,241,0.06)' } },
  'soft-light': { label: 'Soft Light', light: true, vars: { '--bg-primary': '#e8ebef', '--bg-secondary': '#f4f6f8', '--bg-tertiary': '#dce0e6', '--border-color': '#c3c9d2', '--border-light': '#777d89', '--text-primary': '#2b323c', '--text-secondary': '#454d59', '--text-muted': '#5b6370', '--canvas-bg': '#dfe3e8', '--grid-color': 'rgba(43,50,60,0.08)' } },
  'soft-dark': { label: 'Soft Dark', vars: { '--bg-primary': '#22262b', '--bg-secondary': '#2b3036', '--bg-tertiary': '#353b43', '--border-color': '#454c56', '--border-light': '#7c8693', '--text-primary': '#e1e6ec', '--text-secondary': '#b9c1cb', '--text-muted': '#9ca6b2', '--canvas-bg': '#1b1e22', '--grid-color': 'rgba(225,230,236,0.06)' } },
};

export const DEFAULT_THEME = 'studio';
export const DEFAULT_ACCENT = '#f97316';
/** The redesign's first default accent, moved to orange by appearance 3 unless it was chosen on purpose. */
export const OLD_DEFAULT_ACCENT = '#aba0d3';
export const DEFAULT_INTENSITY = 1;
// the app persists UI settings under this key as JSON {theme, accentColor, ...}
export const STORAGE_KEY = 'nodemode_ui_settings';

export interface ThemeChoice { theme: string; accentColor: string; intensity?: number }

/** The appearance generation: a choice saved before Studio Dark (Phase 21) lacks it and is set aside once, so the
 *  redesign reaches everyone. Generation 3 (1.48.1): the accent defaults to orange; a generation-2 choice keeps its
 *  theme and any accent chosen on purpose, the old lavender default becoming orange. index.html's boot script agrees. */
export const APPEARANCE = 3;
const clampIntensity = (v: unknown): number => (typeof v === 'number' && Number.isFinite(v) ? Math.min(1.5, Math.max(0, v)) : DEFAULT_INTENSITY);

export function readChoice(): ThemeChoice {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const saved = JSON.parse(raw) as Partial<ThemeChoice> & { appearance?: number };
      if (saved.appearance === APPEARANCE) return { theme: saved.theme || DEFAULT_THEME, accentColor: saved.accentColor || DEFAULT_ACCENT, intensity: clampIntensity(saved.intensity) };
      if (saved.appearance === 2) return { theme: saved.theme || DEFAULT_THEME, accentColor: !saved.accentColor || saved.accentColor === OLD_DEFAULT_ACCENT ? DEFAULT_ACCENT : saved.accentColor, intensity: DEFAULT_INTENSITY };
    }
  } catch (e) {
    console.error('[theme] read failed:', e);
  }
  return { theme: DEFAULT_THEME, accentColor: DEFAULT_ACCENT, intensity: DEFAULT_INTENSITY };
}

export function saveChoice(choice: ThemeChoice): void {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const merged = { ...(raw ? JSON.parse(raw) : {}), ...choice, appearance: APPEARANCE };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(merged));
  } catch (e) {
    console.error('[theme] save failed:', e);
  }
}

/** Apply a profile and accent to the document. Mirrors applySettings():
 * profile vars first, then the derived --bg-node and --shadow-lg the
 * profiles do not declare, then the accent and its dim variant. Idempotent:
 * writing the same value to a custom property does not trigger a repaint of
 * anything that did not change. */
export function applyTheme(choice: ThemeChoice = readChoice()): void {
  const root = document.documentElement;
  const theme = THEMES[choice.theme] || THEMES[DEFAULT_THEME];
  for (const [k, v] of Object.entries(theme.vars)) root.style.setProperty(k, v);
  document.body.classList.toggle('theme-light', !!theme.light);
  root.style.setProperty('--bg-node', theme.vars['--bg-node'] || theme.vars['--bg-secondary']);
  root.style.setProperty('--shadow-lg', theme.light
    ? '0 4px 14px rgba(96, 90, 70, 0.16)'
    : '0 8px 32px rgba(0, 0, 0, 0.5)');
  root.style.setProperty('--accent-color', choice.accentColor);
  root.style.setProperty('--accent-color-dim', choice.accentColor + '33');
  root.style.setProperty('--accent-color-hover', `color-mix(in srgb, ${choice.accentColor} 82%, #ffffff)`);
  root.style.setProperty('--accent-intensity', String(clampIntensity(choice.intensity)));
  root.style.setProperty('--accent-tint', hexToRgba(choice.accentColor, 0.06));
  root.style.setProperty('--accent-tint-strong', hexToRgba(choice.accentColor, 0.10));
}

export function setTheme(theme: string): void {
  const choice = { ...readChoice(), theme };
  saveChoice(choice);
  applyTheme(choice);
  document.dispatchEvent(new CustomEvent('nm:ui-settings-changed', { detail: { key: 'theme' } }));
}

/** Preferences > Interface: how strongly the accent tints what it marks (0 to 1.5; 1 is the design's). */
export function setIntensity(intensity: number): void {
  const choice = { ...readChoice(), intensity: clampIntensity(intensity) };
  saveChoice(choice);
  applyTheme(choice);
  document.dispatchEvent(new CustomEvent('nm:ui-settings-changed', { detail: { key: 'intensity' } }));
}
export function setAccent(accentColor: string): void {
  const choice = { ...readChoice(), accentColor };
  saveChoice(choice);
  applyTheme(choice);
  document.dispatchEvent(new CustomEvent('nm:ui-settings-changed', { detail: { key: 'accentColor' } }));
}

/** Swatch markup for a theme picker, the app's .nm-theme-swatch recipe:
 * background is the profile's --bg-secondary, the active swatch borders in
 * the accent, every other swatch in its own --border-color. */
export function themeSwatchesHtml(active: string = readChoice().theme, accent: string = readChoice().accentColor): string {
  return Object.entries(THEMES).map(([id, th]) =>
    `<button class="nm-theme-swatch ${active === id ? 'is-active' : ''}" title="${th.label}" data-theme-id="${id}"` +
    ` style="background:${th.vars['--bg-secondary']};border-color:${active === id ? accent : th.vars['--border-color']}"></button>`
  ).join('');
}

function hexToRgba(hex: string, alpha: number): string {
  const h = hex.replace('#', '');
  const full = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  const n = parseInt(full.slice(0, 6), 16);
  if (Number.isNaN(n)) return `rgba(171,160,211,${alpha})`;
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`;
}
