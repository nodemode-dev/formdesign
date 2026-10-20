const OLD = 'formdes.';
const NEW = 'formdesign.';

export function migrateStorage(): void {
  try {
    const keys: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k !== null && k.startsWith(OLD)) keys.push(k);
    }
    for (const k of keys) {
      const next = NEW + k.slice(OLD.length);
      const value = localStorage.getItem(k);
      if (value !== null && localStorage.getItem(next) === null) localStorage.setItem(next, value);
      localStorage.removeItem(k);
    }
  } catch { }
}
