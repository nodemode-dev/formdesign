export type DockSide = 'bottom' | 'right';
export interface DockState { open: boolean; side: DockSide; h: number; w: number; tab: string }
export interface DockStore { getItem(key: string): string | null; setItem(key: string, value: string): void; removeItem(key: string): void }

export interface DockLegacy { keys: string[]; read(store: DockStore): (Partial<DockState> & { seen?: boolean }) | null }

export function jsonLegacy(keys: string[], seen = false): DockLegacy {
  return {
    keys,
    read: (store) => {
      for (const k of keys) {
        try { const v = store.getItem(k); const s: unknown = v ? JSON.parse(v) : null; if (s && typeof s === 'object') return { ...(s as Partial<DockState>), seen }; } catch { continue; }
      }
      return null;
    },
  };
}

export function readState(store: DockStore, key: string, from: DockLegacy | string[] | undefined, defaults: { h: number; w: number }): DockState {
  const legacy = Array.isArray(from) ? jsonLegacy(from) : from;
  const own = (): Partial<DockState> | null => {
    try { const v = store.getItem(key); const s: unknown = v ? JSON.parse(v) : null; return s && typeof s === 'object' ? s as Partial<DockState> : null; } catch { return null; }
  };
  const old = (): Partial<DockState> | null => { try { return legacy ? legacy.read(store) : null; } catch { return null; } };
  const s = own() ?? old() ?? {};
  return {
    open: s.open === true, side: s.side === 'right' ? 'right' : 'bottom',
    h: typeof s.h === 'number' ? s.h : defaults.h, w: typeof s.w === 'number' ? s.w : defaults.w, tab: typeof s.tab === 'string' ? s.tab : '',
  };
}

export function readSeen(store: DockStore, key: string): boolean {
  try { const v = store.getItem(key); return v ? (JSON.parse(v) as { seen?: unknown }).seen === true : false; } catch { return false; }
}

export function moveLegacy(store: DockStore, key: string, from: DockLegacy | string[] | undefined, defaults: { h: number; w: number }): boolean {
  const legacy = Array.isArray(from) ? jsonLegacy(from) : from;
  try {
    if (!legacy || store.getItem(key) !== null || !legacy.keys.some((k) => store.getItem(k) !== null)) return false;
    const state = readState(store, key, legacy, defaults);
    store.setItem(key, JSON.stringify(legacy.read(store)?.seen === true ? { ...state, seen: true } : state));
    for (const k of legacy.keys) store.removeItem(k);
    return true;
  } catch { return false; }
}
