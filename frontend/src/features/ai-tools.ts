import { api } from '../engine/client';
import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { enhanceSelect } from '../ui/dropdown';
import { el, patchList, q, setDisabled, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface AltRow { id: number; asset: number; name: string; current: string; proposed: string; error: string; thumb: string }
interface AltPlan { scope: 'selected' | 'missing'; items: AltRow[]; total: number }

const nice = (m: string): string => m.replace(/^Error: /, '');

/** Object > Generate Alt Text: the model proposes, the person edits and accepts, then the text is written to each frame. */
class AltTextDialog {
  private readonly d = new Dialog('Generate Alt Text', 640);
  private readonly note = el('<p class="fa-dnote"></p>');
  private readonly list = el('<div class="fa-alts"></div>');
  private readonly write: HTMLButtonElement;
  private rows: AltRow[] = [];
  private seq = 0;

  constructor() {
    this.d.body.append(this.note, this.list);
    this.d.button('Cancel', '', () => this.d.close());
    this.write = this.d.button('Write Alt Text', 'primary', () => void this.apply());
  }

  async open(): Promise<void> {
    const mine = ++this.seq;
    setText(this.note, 'Reading the images...');
    setDisabled(this.write, true);
    this.list.replaceChildren();
    this.d.open();
    try {
      const plan = await api<AltPlan>('/ai/alt-text/propose', { json: {} });
      if (mine !== this.seq) return;
      this.rows = plan.items;
      if (!plan.items.length) {
        setText(this.note, plan.total ? 'Every image already has alt text. Select images to write it again.' : 'There are no images in this document.');
        return;
      }
      setText(this.note, plan.scope === 'selected' ? 'The selected images. Edit any text, untick what you do not want, then write.' : 'Images without alt text. Edit any text, untick what you do not want, then write.');
      patchList<AltRow>(this.list, plan.items, {
        key: (r) => String(r.id),
        create: (r) => {
          const n = el(`<div class="fa-alt"><label class="fa-alt-pick"><input type="checkbox" checked aria-label="Include"></label><img class="fa-alt-img" alt=""><div class="fa-alt-main"><span class="fa-alt-name"></span><textarea class="cv-input fa-alt-text" rows="2" aria-label="Alt text"></textarea><span class="fa-alt-err"></span></div></div>`);
          const img = q<HTMLImageElement>(n, '.fa-alt-img');
          if (r.thumb) img.src = r.thumb;
          setText(q(n, '.fa-alt-name'), `${r.name}${r.current ? ' (has alt text)' : ''}`);
          q<HTMLTextAreaElement>(n, '.fa-alt-text').value = r.proposed || r.current;
          setText(q(n, '.fa-alt-err'), r.error);
          if (r.error) { q<HTMLInputElement>(n, 'input').checked = false; }
          return n;
        },
        update: () => undefined,
      });
      setDisabled(this.write, false);
    } catch (e) { if (mine === this.seq) setText(this.note, nice((e as Error).message)); }
  }

  private async apply(): Promise<void> {
    const items = Array.from(this.list.children).map((n) => ({
      id: Number((n as HTMLElement).dataset.key), on: q<HTMLInputElement>(n as HTMLElement, 'input').checked, text: q<HTMLTextAreaElement>(n as HTMLElement, '.fa-alt-text').value.trim(),
    })).filter((i) => i.on && i.text);
    if (!items.length) { toast('Nothing to write'); return; }
    setDisabled(this.write, true);
    try {
      const r = await api<{ written: number }>('/ai/alt-text/apply', { json: { items: items.map((i) => ({ id: i.id, text: i.text })) } });
      toast(`Wrote alt text for ${r.written} image${r.written === 1 ? '' : 's'}`);
      this.d.close();
    } catch (e) { toast(nice((e as Error).message), 'error'); setDisabled(this.write, false); }
  }
}

const STYLES = ['none', 'photo', 'illustration', 'watercolor', 'flat vector', '3D render', 'line art', 'cinematic'];

/** Object > Generate Image: a prompt and a style; the picture is sized from the selected frame and placed into it. */
class GenerateImageDialog {
  private readonly d = new Dialog('Generate Image', 480);
  private readonly prompt = el<HTMLTextAreaElement>('<textarea class="cv-input fa-prompt" rows="4" placeholder="Describe the picture" aria-label="Prompt"></textarea>');
  private readonly style = el<HTMLSelectElement>('<select class="cv-select" aria-label="Style"></select>');
  private readonly info = el('<p class="fa-dnote"></p>');
  private readonly go: HTMLButtonElement;

  constructor() {
    this.style.append(...STYLES.map((s) => new Option(s === 'none' ? 'No style' : s[0].toUpperCase() + s.slice(1), s)));
    this.d.field('Prompt', this.prompt);
    this.d.field('Style', this.style);
    enhanceSelect(this.style);
    this.d.body.appendChild(this.info);
    this.d.button('Cancel', '', () => this.d.close());
    this.go = this.d.button('Generate', 'primary', () => void this.run());
  }

  open(): void {
    const ids = editor.selection?.items ?? [];
    setText(this.info, ids.length ? 'The picture is made at the selected frame’s shape and placed into it, and saved in the document’s links folder.' : 'Select a graphic frame first; the picture is placed into it.');
    setDisabled(this.go, false);
    setText(this.go, 'Generate');
    this.d.open(this.prompt);
  }

  private async run(): Promise<void> {
    if (!this.prompt.value.trim()) { toast('Describe the picture first', 'error'); return; }
    setDisabled(this.go, true);
    setText(this.go, 'Generating...');
    try {
      const r = await api<{ path: string }>('/ai/image/generate', { json: { prompt: this.prompt.value, style: this.style.value } });
      toast(`Placed a generated picture (${r.path.split('/').pop()})`);
      this.d.close();
    } catch (e) { toast(nice((e as Error).message), 'error'); setDisabled(this.go, false); setText(this.go, 'Generate'); }
  }
}

/** Object > AI Expand: the placed picture is extended to fill its frame's shape. */
class ExpandDialog {
  private readonly d = new Dialog('AI Expand', 460);
  private readonly prompt = el<HTMLTextAreaElement>('<textarea class="cv-input fa-prompt" rows="3" placeholder="Optional: what should appear in the new area" aria-label="Prompt"></textarea>');
  private readonly go: HTMLButtonElement;

  constructor() {
    this.d.body.append(el('<p class="fa-dnote">The picture in the selected frame is extended, with the model filling the empty area, until it fits the frame’s shape. The result is saved in the links folder and replaces the picture in the frame.</p>'));
    this.d.field('Prompt', this.prompt);
    this.d.button('Cancel', '', () => this.d.close());
    this.go = this.d.button('Expand', 'primary', () => void this.run());
  }

  open(): void { setDisabled(this.go, false); setText(this.go, 'Expand'); this.d.open(this.prompt); }

  private async run(): Promise<void> {
    setDisabled(this.go, true);
    setText(this.go, 'Expanding...');
    try {
      await api('/ai/image/expand', { json: { prompt: this.prompt.value } });
      toast('Extended the picture to fill the frame');
      this.d.close();
    } catch (e) { toast(nice((e as Error).message), 'error'); setDisabled(this.go, false); setText(this.go, 'Expand'); }
  }
}

export function installAiTools(ws: Workspace): void {
  let alt: AltTextDialog | null = null, gen: GenerateImageDialog | null = null, exp: ExpandDialog | null = null;
  const has = (): boolean => editor.hasDoc;
  ws.addCommands(
    { id: 'ui.ai.altText', label: 'Generate Alt Text…', enabled: has, run: () => void (alt ??= new AltTextDialog()).open() },
    { id: 'ui.ai.generateImage', label: 'Generate Image…', enabled: has, run: () => (gen ??= new GenerateImageDialog()).open() },
    { id: 'ui.ai.expandImage', label: 'AI Expand…', enabled: has, run: () => (exp ??= new ExpandDialog()).open() },
    {
      id: 'ui.ai.autoStyle', label: 'Auto Style', enabled: has, run: () => {
        toast('Styling the text...');
        void api<{ applied: number; created: string[]; mode: string; note?: string }>('/ai/auto-style', { json: {} }).then((r) => {
          const how = r.mode === 'ai' ? 'with the model' : r.note ? `by rules (${r.note})` : 'by rules';
          toast(r.applied ? `Styled ${r.applied} paragraph${r.applied === 1 ? '' : 's'} ${how}${r.created.length ? `; added ${r.created.join(', ')}` : ''}. Undo reverses it.` : 'No paragraphs needed a style');
        }).catch((e) => toast(nice((e as Error).message), 'error'));
      },
    },
  );
  ws.addMenuItems({ menu: 'Object', items: ['-', 'ui.ai.altText', 'ui.ai.generateImage', 'ui.ai.expandImage'], at: 'end' });
  ws.addMenuItems({ menu: 'Type', items: ['-', 'ui.ai.autoStyle'], at: 'end' });
}
