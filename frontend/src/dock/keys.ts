import type { DockAdapter } from './adapter';

export type DockKeys = Pick<DockAdapter, 'passKey' | 'isDockChord' | 'chordLabel'>;

export function onMac(): boolean {
  return /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
}

export function isChord(a: DockKeys, e: KeyboardEvent): boolean {
  if (a.isDockChord) return a.isDockChord(e);
  if (e.code !== 'KeyJ' || e.shiftKey || e.altKey) return false;
  return onMac() ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}

export function chordLabel(a: DockKeys): string {
  return a.chordLabel ? a.chordLabel() : onMac() ? 'Cmd+J' : 'Ctrl+J';
}

export function leaves(a: DockKeys, e: KeyboardEvent): boolean {
  return isChord(a, e) || a.passKey?.(e) === true;
}

export function contain(node: HTMLElement, a: DockKeys): void {
  const stop = (e: Event): void => { if (!leaves(a, e as KeyboardEvent)) e.stopPropagation(); };
  for (const type of ['keydown', 'keypress', 'keyup']) node.addEventListener(type, stop);
}

export function ownsEvent(e: Event): boolean {
  const at = e.target;
  return at instanceof Element && at.closest('.ft-dock, .ft-consent, .ft-tour') !== null;
}

export function keeps(a: DockKeys, e: Event): boolean {
  return ownsEvent(e) && !(e instanceof KeyboardEvent && leaves(a, e));
}
