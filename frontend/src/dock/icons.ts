const ICONS: Record<string, string> = {
  plus: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 6v12M6 12h12"/></svg>`,
  close: `<svg viewBox="0 0 24 24" fill="currentColor"><path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/></svg>`,
  refresh: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>`,
  send: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>`,
  agent: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><g transform="translate(0 .9)"><rect x="4" y="7.5" width="16" height="12" rx="4"/><path d="M12 7.5V4.5"/><circle cx="12" cy="3.6" r="1.1"/><circle cx="9.3" cy="13.3" r="1.35" fill="currentColor" stroke="none"/><circle cx="14.7" cy="13.3" r="1.35" fill="currentColor" stroke="none"/><path d="M2 12.2v2.8M22 12.2v2.8"/></g></svg>`,
  agentChat: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4.5h12a3.5 3.5 0 0 1 3.5 3.5v6a3.5 3.5 0 0 1-3.5 3.5h-6.5L7 21v-3.5H6A3.5 3.5 0 0 1 2.5 14V8A3.5 3.5 0 0 1 6 4.5z"/><circle cx="9.2" cy="11" r="1.3" fill="currentColor" stroke="none"/><circle cx="14.8" cy="11" r="1.3" fill="currentColor" stroke="none"/></svg>`,
  terminal: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="M7 9.5l3 2.5-3 2.5M12.5 15h4.5"/></svg>`,
  codex: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M8.5 6.5L3 12l5.5 5.5M15.5 6.5L21 12l-5.5 5.5"/><circle cx="12" cy="12" r="1.6" fill="currentColor" stroke="none"/></svg>`,
  cursor: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 4.5h5M8 4.5v15M5.5 19.5h5"/><path d="M13.5 10.5l7 2.8-3 1.2-1.2 3z"/></svg>`,
  dockRight: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="M14.5 4.5v15"/><path d="M8 12h3.5M10 10l1.75 2L10 14"/></svg>`,
  dockBottom: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="M3 14.5h18"/><path d="M12 7.5V11M10 9.5l2 1.75 2-1.75"/></svg>`,
  menu: `<svg viewBox="0 0 24 24" fill="currentColor" stroke="none"><circle cx="12" cy="5.5" r="1.6"/><circle cx="12" cy="12" r="1.6"/><circle cx="12" cy="18.5" r="1.6"/></svg>`,
};

export type DockIcon = keyof typeof ICONS;

export function dockIcon(name: string, size = 16): string {
  const svg = ICONS[name] ?? ICONS.terminal;
  return `<span class="ft-icon ft-icon-${size}" aria-hidden="true">${svg}</span>`;
}
