import '../styles/story.css';
import { editor } from '../editor/editor';
import type { ChangeEvent } from '../engine/client';
import { el, q, setHidden, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface StoryRow { id: number; frames: number[]; length: number; paragraphs: number; overset: boolean | null; preview: string }
interface StoryText { id: number; text: string; frames: number[]; paragraphs: number; overset: boolean | null }

const utf8 = new TextEncoder();
const bytes = (s: string) => utf8.encode(s).length;

class StoryEditor {
  private readonly root = el(`<section class="fd-story" aria-label="Story Editor" hidden>
    <div class="fd-story-head">
      <span class="fd-story-title">Story Editor</span>
      <select class="cv-select fd-story-pick" aria-label="Story"></select>
      <span class="fd-story-meta"></span>
      <button class="cv-icon-btn fd-story-close" type="button" title="Close the Story Editor" aria-label="Close">${icon('close', 16)}</button>
    </div>
    <textarea class="fd-story-text" spellcheck="true" aria-label="Story text"></textarea>
    <div class="fd-story-foot"><span class="fd-story-hint">Edits flow into the layout as you type; each line is a paragraph and keeps its style.</span><span class="fd-story-over" hidden>Overset: the story does not fit its frames</span></div>
  </section>`);
  private readonly text = q<HTMLTextAreaElement>(this.root, '.fd-story-text');
  private readonly pick = q<HTMLSelectElement>(this.root, '.fd-story-pick');
  private story: number | null = null;
  private base = '';
  private timer = 0;
  private reloadTimer = 0;
  private saving = false;

  constructor(private readonly ws: Workspace) {
    ws.viewport.stage.appendChild(this.root);
    q(this.root, '.fd-story-close').addEventListener('click', () => this.close());
    this.pick.addEventListener('change', () => void this.load(Number(this.pick.value)));
    this.text.addEventListener('input', () => { window.clearTimeout(this.timer); this.timer = window.setTimeout(() => void this.flush(), 450); });
    this.text.addEventListener('blur', () => void this.flush());
    this.text.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); this.close(); }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'y') { e.preventDefault(); this.close(); }
    });
    editor.on('change', (e: ChangeEvent) => { if (this.isOpen && !this.saving && e?.docChanged) this.scheduleReload(); });
    editor.on('docs', () => { if (this.isOpen) void this.open(); });
  }

  get isOpen(): boolean { return !this.root.hidden; }

  toggle(): void { if (this.isOpen) this.close(); else void this.open(); }

  async open(): Promise<void> {
    if (!editor.hasDoc) { toast('Open a document to edit its stories'); return; }
    const ins = await editor.inspect().catch(() => null) as { stories?: StoryRow[] } | null;
    const stories = (ins?.stories ?? []).filter((s) => s.length > 0 || s.frames.length);
    if (!stories.length) { toast('This document has no text yet'); return; }
    this.pick.replaceChildren(...stories.map((s) => new Option(`${s.preview.slice(0, 48) || '(empty)'}${s.overset ? '  · overset' : ''}`, String(s.id))));
    const sel = editor.selection;
    const t = sel?.text as { story?: number } | undefined;
    const frame = sel?.items?.[0];
    const want = t?.story ?? stories.find((s) => frame != null && s.frames.includes(frame))?.id ?? this.story ?? stories.sort((a, b) => b.length - a.length)[0].id;
    setHidden(this.root, false);
    await this.load(stories.some((s) => s.id === want) ? want : stories[0].id);
    this.text.focus();
  }

  close(): void {
    void this.flush();
    setHidden(this.root, true);
    this.ws.viewport.refreshOverlays();
  }

  private scheduleReload(): void {
    window.clearTimeout(this.reloadTimer);
    this.reloadTimer = window.setTimeout(() => { if (document.activeElement !== this.text && this.story != null) void this.load(this.story); }, 250);
  }

  private async load(id: number): Promise<void> {
    await this.flush();
    const s = await editor.exec<StoryText>('story.get', { story: id }, true).catch(() => undefined);
    if (!s) return;
    this.story = id;
    this.pick.value = String(id);
    this.base = s.text ?? '';
    if (this.text.value !== this.base) this.text.value = this.base;
    this.meta(s);
  }

  private meta(s: { text?: string; paragraphs?: number; overset?: boolean | null }): void {
    const words = (s.text ?? '').split(/\s+/).filter(Boolean).length;
    setText(q(this.root, '.fd-story-meta'), `${words} words · ${s.paragraphs ?? 0} paragraphs`);
    setHidden(q(this.root, '.fd-story-over'), !s.overset);
  }

  private async flush(): Promise<void> {
    window.clearTimeout(this.timer);
    if (this.story == null || this.saving) return;
    const next = this.text.value;
    const prev = this.base;
    if (next === prev) return;
    let a = 0;
    while (a < prev.length && a < next.length && prev[a] === next[a]) a++;
    let b = 0;
    while (b < prev.length - a && b < next.length - a && prev[prev.length - 1 - b] === next[next.length - 1 - b]) b++;
    const start = bytes(prev.slice(0, a));
    const end = bytes(prev.slice(0, prev.length - b));
    this.saving = true;
    try {
      const r = await editor.exec('story.replaceRange', { story: this.story, start, end, text: next.slice(a, next.length - b) });
      if (r !== undefined) this.base = next;
      const s = await editor.exec<StoryText>('story.get', { story: this.story }, true).catch(() => undefined);
      if (s) this.meta(s);
    } finally {
      this.saving = false;
    }
    this.ws.viewport.refreshOverlays();
  }
}

export function installStoryEditor(ws: Workspace): void {
  let se: StoryEditor | null = null;
  ws.addCommands({ id: 'ui.storyEditor', label: 'Edit in Story Editor', keys: ['Mod+Y'], enabled: () => editor.hasDoc, run: () => (se ??= new StoryEditor(ws)).toggle() });
  ws.addMenuItems({ menu: 'Edit', items: ['ui.storyEditor'], at: 'end' });
}
