import type { DockLegacy } from './state';
import type { StringKey } from './strings';

export type { DockLegacy, DockSide, DockState, DockStore } from './state';
export type DockKind = 'shell' | 'claude' | 'codex' | 'cursor' | 'chat';
export type DockProgram = 'claude' | 'codex' | 'cursor';
export type PtyKind = 'shell' | 'claude' | 'codex' | 'cursor' | 'agent';
export type ChatMode = 'ask' | 'auto-edits' | 'plan';

export interface DockTerminal<K extends string = never> { id: string; title: string; alive: boolean; kind: PtyKind | K; tip?: string; status?: string }
export interface DockChannel {
  send(data: string): void;
  resize(cols: number, rows: number): void;
  onData(handler: (data: string | Uint8Array) => void): void;
  onExit(handler: () => void): void;
  onClose?(handler: () => void): void;
  onStatus?(handler: (text: string) => void): void;
  close(): void;
}
export interface DockOpen { cols: number; rows: number; prompt?: string; resume?: string }
export interface DockProgramStatus { installed: boolean; version: string | null; install: string; signedIn: boolean | null; label?: string; note?: string }
export type DockAgentStatus<K extends string = never> = Partial<Record<DockProgram | K, DockProgramStatus>>;
export interface DockChatStatus {
  installed: boolean;
  version: string | null;
  signedIn: boolean | null;
  install: string;
  running: boolean;
  state: string;
  session: string | null;
  mode?: ChatMode;
  lead?: string;
  refused?: string;
}
export interface DockChatHandlers { open(): void; event(event: DockChatEvent): void; close(): void }
export interface DockChatEngine {
  status(): Promise<DockChatStatus | null>;
  send(text: string): Promise<void>;
  subscribe(handlers: DockChatHandlers): () => void;
  start?(mode: ChatMode | null, fresh: boolean): Promise<void>;
  stop?(): Promise<void>;
  answer?(id: string, allow: boolean): Promise<void>;
  modes?: ChatMode[];
  renderText?(node: HTMLElement, text: string): void;
}
export type DockSent = boolean | void | Promise<boolean | void>;
export type DockPaneHidden = 'tab' | 'dock';
export interface DockOwnChat {
  send?(text: string): DockSent;
  focus?(): void;
  shown?(): void;
  hidden?(why: DockPaneHidden): void;
  closed?(): void;
  busy?(): boolean;
  askTarget?: HTMLElement;
}
export interface DockOwnAsk<K extends string = never> { send?(text: string, tab: DockKind | K | null): DockSent; element?: HTMLElement }
export interface DockChatPane<K extends string = never> { pane(host: HTMLElement): DockOwnChat | void; ask?: 'none' | 'line' | DockOwnAsk<K> }
export type DockChatEvent =
  | { type: 'state'; running: boolean; state: string; session: string | null; mode?: ChatMode }
  | { type: 'user'; text: string; note?: string }
  | { type: 'reset' }
  | { type: 'meter'; text: string; high?: boolean }
  | { type: 'block'; id: string; kind: 'text' | 'thinking' }
  | { type: 'delta'; id: string; text: string }
  | { type: 'tool'; id: string; name: string; summary: string }
  | { type: 'tool_result'; id: string; text: string; error: boolean }
  | { type: 'permission'; id: string; tool: string; summary: string }
  | { type: 'permission_done'; id: string; allowed: boolean }
  | { type: 'result'; ok: boolean; text: string; meta?: string }
  | { type: 'notice'; text: string };

export interface DockSizes {
  minHeight: number;
  minWidth: number;
  defaultHeight: number;
  defaultWidth: number;
  maxHeightShare: number;
  maxWidthShare: number;
  keepHeight?: number;
  keepWidth?: number;
  maxHeight?(): number;
  maxWidth?(): number;
}
export type DockErrorKind = 'refused' | 'unreachable' | 'notInstalled' | 'oldEngine' | 'busy';
export interface DockWords { app: string; noun: string; lead: string; example: string; start?: string }
export interface DockAppTab { id: string; label: string; icon: string; mount(host: HTMLElement): void | (() => void) }
export interface DockTabs<K extends string = never> { builtin: Array<DockKind | K>; own?: DockAppTab[] }
export interface DockStorage { key: string; legacy?: DockLegacy | string[] }
export type DockPlacement =
  | { mode: 'own'; storage: DockStorage; sizes: DockSizes; canDockRight?(): boolean }
  | { mode: 'hosted'; request(open: boolean): void };

export interface DockAdapter<K extends string = never> {
  words: DockWords;
  strings?: Partial<Record<StringKey, string>>;
  placement: DockPlacement;
  passKey?(event: KeyboardEvent): boolean;
  isDockChord?(event: KeyboardEvent): boolean;
  chordLabel?(): string;
  tabs(): DockTabs<K>;
  classes: { iconButton: string; button: string; primaryButton: string; input: string; select: string; note: string };
  t(key: string, english: string): string;
  onTheme(handler: () => void): void;
  keyHeaders?(): Record<string, string>;
  errors: { message(err: unknown): string | null; kind?(err: unknown): DockErrorKind | null };
  firstRun?(): boolean;
  terminal: {
    list(): Promise<DockTerminal<K>[]>;
    open(kind: Exclude<PtyKind, 'agent'> | K, options: DockOpen): Promise<DockTerminal<K>>;
    restart?(id: string): Promise<void>;
    close(id: string): Promise<void>;
    channel(id: string): DockChannel | Promise<DockChannel>;
    scrollback?: number;
  };
  agentStatus(): Promise<DockAgentStatus<K> | null>;
  chat?: DockChatEngine | DockChatPane<K>;
  consent?: {
    subscribe(add: (request: ConsentRequest) => void, withdraw: (id: string) => void): void;
    answer(id: string, choice: ConsentChoice): Promise<void>;
  };
  storedChoice?: {
    get(): Promise<StoredChoiceState>;
    set(value: StoredChoice): Promise<void>;
  };
  openSettings?(): void;
}

export const DOCK_PUBLIC = {
  classes: ['ft-dock', 'ft-consent', 'ft-tour', 'ft-term', 'ft-chat', 'ft-dock-pane', 'ft-notranslate', 'is-hosted', 'has-own-chat', 'has-no-ask', 'has-ask-line', 'ft-dock-ask', 'ft-dock-ask-own', 'ft-dock-head', 'ft-dock-tab', 'ft-dock-stage', 'ft-icon', 'is-on', 'is-own'],
  rootAttributes: ['data-ft-dock', 'data-ft-dock-side', 'data-ft-dock-h', 'data-ft-dock-w'],
  partAttribute: 'data-dock-part',
  parts: ['new', 'chat', 'launch.claude', 'more', 'restart', 'side', 'hide', 'menu', 'menu.settings', 'menu.guide', 'screen', 'note.copy', 'note.dismiss', 'ask', 'send', 'tab.chat', 'tab.close', 'consent', 'consent.deny', 'consent.once', 'consent.always'],
  partsByKind: ['launch.', 'menu.launch.', 'tab.'],
  rootProperties: ['--ft-dock-size', '--ft-dock-wsize'],
  tokens: [
    '--dock-bg', '--dock-bar-bg', '--dock-raised', '--dock-hover', '--dock-input-bg', '--dock-border', '--dock-edge', '--dock-divider', '--dock-tab-border', '--dock-outline', '--dock-scrim',
    '--dock-text', '--dock-text-dim', '--dock-text-muted', '--dock-tab-text', '--dock-accent', '--dock-accent-line', '--dock-accent-ink', '--dock-danger', '--dock-danger-text',
    '--dock-space-1', '--dock-space-2', '--dock-space-3', '--dock-space-4', '--dock-radius-xs', '--dock-radius-sm', '--dock-radius', '--dock-radius-md', '--dock-tab-radius',
    '--dock-head-h', '--dock-tab-h', '--dock-ibtn', '--dock-ask-h', '--dock-chat-bar-h', '--dock-ctl-h', '--dock-icon', '--dock-icon-sm', '--dock-icon-xs', '--dock-icon-note', '--dock-icon-send',
    '--dock-stage-pad', '--dock-ask-ctl-h', '--dock-tab-accent',
    '--dock-fs-xs', '--dock-fs-sm', '--dock-fs-md', '--dock-font-mono',
    '--dock-term-bg', '--dock-term-fg', '--dock-term-cursor', '--dock-term-selection', '--dock-term-font-size', '--dock-term-line-height', '--dock-term-contrast',
  ],
  layers: ['--z-raised', '--z-overlay'],
} as const;

export type ConsentChoice = 'deny' | 'once' | 'always';
export type ConsentScope = 'agent' | 'all';
export type StoredChoice = 'ask' | 'allowed' | 'blocked';
export interface ConsentRequest {
  id: string;
  agent: string;
  what: string;
  offers: ConsentChoice[];
  always: ConsentScope;
  expires?: number;
  escape?: 'deny';
  notes?: string[];
}
export interface StoredChoiceState { value: StoredChoice; changedOutside: boolean }
export type ConsentHost = Pick<DockAdapter, 'words' | 'strings' | 't' | 'classes' | 'consent' | 'storedChoice'>;
