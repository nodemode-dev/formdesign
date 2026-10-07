import type { Pack, Report } from './api';

export interface MadeDoc { report: Report; pack: Pack | null }

export const madeDocs = new Map<number, MadeDoc>();

export const links = {
  revealSlot: (_slot: string): void => undefined,
  pickTemplate: (): Promise<string | null> => Promise.resolve(null),
  openBrands: (_name?: string): void => undefined,
};

export function friendlySlot(slot: string): string {
  if (slot.startsWith('flow:')) return slot.slice(5) === 'body' ? 'body text' : `${slot.slice(5)} text`;
  return slot;
}
