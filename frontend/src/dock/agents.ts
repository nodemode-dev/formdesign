import type { StringKey } from './strings';

export interface DockAgentKind { id: string; label: string; name?: StringKey; icon: string; ready: StringKey; missing: StringKey }

export const DOCK_AGENTS: readonly DockAgentKind[] = [
  { id: 'claude', label: 'Claude Code', name: 'claudeCode', icon: 'agent', ready: 'programReady', missing: 'notInstalled' },
  { id: 'codex', label: 'Codex', name: 'codex', icon: 'codex', ready: 'programReady', missing: 'programMissing' },
  { id: 'cursor', label: 'Cursor', name: 'cursor', icon: 'cursor', ready: 'programReady', missing: 'programMissing' },
  { id: 'antigravity', label: 'Antigravity', name: 'antigravity', icon: 'antigravity', ready: 'programReady', missing: 'programMissing' },
];

export const DOCK_PLAIN: readonly string[] = ['shell', 'chat', 'agent'];

export function agentKind(id: string): DockAgentKind {
  return DOCK_AGENTS.find((k) => k.id === id) ?? { id, label: id, icon: id, ready: 'programReady', missing: 'programMissing' };
}

export function agentKinds(builtin: readonly string[]): DockAgentKind[] {
  const listed = DOCK_AGENTS.filter((k) => builtin.includes(k.id));
  const more = builtin.filter((id) => !DOCK_PLAIN.includes(id) && !DOCK_AGENTS.some((k) => k.id === id)).map(agentKind);
  return [...listed, ...more];
}
