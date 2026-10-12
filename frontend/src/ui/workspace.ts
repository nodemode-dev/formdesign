import type { Editor } from '../editor/editor';
import type { Input } from '../editor/input';
import type { Viewport } from '../editor/viewport';
import type { Command, Commands } from './commands';
import type { PanelLayout } from './dock';
import type { MenuItem } from './menu';

export type Column = 'left' | 'side' | 'dock';
export type MenuName = 'File' | 'Edit' | 'Layout' | 'Type' | 'Object' | 'Table' | 'View' | 'Window' | 'Help';

export interface PanelSpec {
  id: string;
  label: string;
  group: string;
  column: Column;
  order?: number;
  create: () => HTMLElement;
  onShow?: () => void;
}

export interface MenuAddition { menu: MenuName; items: MenuItem[]; at?: 'top' | 'start' | 'end' | string }

export interface Workspace {
  readonly editor: Editor;
  readonly viewport: Viewport;
  readonly input: Input;
  readonly commands: Commands;
  readonly root: HTMLElement;
  registerPanel(spec: PanelSpec): void;
  showPanel(id: string): void;
  addCommands(...cmds: Command[]): void;
  addMenuItems(add: MenuAddition): void;
  override(coreId: string, run: (params?: Record<string, unknown>) => void): void;
  runCore(coreId: string, params?: Record<string, unknown>): Promise<unknown>;
  setControl(id: string, node: HTMLElement | null, priority?: number): void;
  setHint(text: string): void;
  pickFiles(accept: string, multiple?: boolean): Promise<File[]>;
  upload(files: File[]): Promise<string[]>;
  showHome(): void;
  onDropFiles(handler: (files: File[]) => boolean): void;
  layout(): PanelLayout;
  applyLayout(layout: PanelLayout): void;
  menuAreas(): Map<string, string>;
}

export interface Feature { install(ws: Workspace): void }
