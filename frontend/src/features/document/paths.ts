import { api } from '../../engine/client';

export interface Capabilities {
  version?: string;
  dataDir?: string;
  paths?: { projects?: string; exports?: string; templates?: string };
  core?: { binary?: string | null; version?: string };
  mcp?: string;
}

let caps: Promise<Capabilities> | null = null;

export function capabilities(): Promise<Capabilities> {
  caps ??= api<Capabilities>('/capabilities').catch(() => { caps = null; return {}; });
  return caps;
}

export function sep(dir: string): string { return dir.includes('\\') && !dir.includes('/') ? '\\' : '/'; }

export function join(dir: string, name: string): string {
  if (!dir) return name;
  const s = sep(dir);
  return dir.replace(/[\\/]+$/, '') + s + name;
}

export function safeName(s: string): string {
  return s.replace(/[\\/:*?"<>|]+/g, '-').replace(/\s+/g, ' ').trim() || 'Untitled';
}

export function stem(title: string | null | undefined): string {
  return safeName((title || 'Untitled').replace(/\.(fdes|fdoc|idml|pdf)$/i, ''));
}

/** The folder holding the formdes launcher: the core binary sits in <repo>/native/target/... */
export function repoDir(c: Capabilities): string {
  const bin = c.core?.binary ?? '';
  const m = bin.match(/^(.*?)[\\/]native[\\/]/);
  return m ? m[1] : '';
}
