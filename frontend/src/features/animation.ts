import '../styles/review.css';
import '../styles/interactive.css';
import '../styles/motion.css';
import { editor } from '../editor/editor';
import { api, engine } from '../engine/client';
import { enhanceSelect } from '../ui/dropdown';
import { el, patchList, q, setChecked, setClass, setDisabled, setHidden, setText, setValue, swapMedia } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { LivePanel } from './review';

type Rect4 = [number, number, number, number];
interface Anim {
  preset: string; event: string; button?: number; duration: number; delay: number; repeat: number; loop: boolean; speed: string;
  opacity: number | null; scale: number | null; rotate: number | null; hideBefore: boolean;
  kf: Keyframe[]; opt: { duration: number; delay: number; iterations: number | 'Infinity'; easing: string; fill: string };
  page: number; pageName: string; label: string; kind: string; bounds: Rect4; spread: number;
}
interface MotionData {
  presets: Array<{ id: string; label: string; enter: boolean }>; events: string[]; speeds: string[];
  anims: Record<string, Anim>; timing: Record<string, number[][]>; items: Record<string, { label: string; kind: string; page: number; pageName: string }>;
}
const EMPTY: MotionData = { presets: [], events: [], speeds: [], anims: {}, timing: {}, items: {} };
const EVENT_LABEL: Record<string, string> = { load: 'On Page Load', click: 'On Click (Self)', button: 'On Click (Button)', hover: 'On Roll Over (Self)' };
const SPEED_LABEL: Record<string, string> = { preset: 'From Preset', none: 'None', easeIn: 'Ease In', easeOut: 'Ease Out', easeInOut: 'Ease In and Out' };
const docRev = (): number => (editor.doc as unknown as { revision?: number } | null)?.revision ?? 0;

function patchOptions(sel: HTMLSelectElement, opts: Array<[string, string]>): void {
  const have = new Map(Array.from(sel.options).map((o) => [o.value, o]));
  const want = new Set(opts.map(([v]) => v));
  for (const [v, o] of have) if (!want.has(v)) o.remove();
  opts.forEach(([v, l], i) => {
    let o = have.get(v);
    if (!o) { o = new Option(l, v); sel.add(o, sel.options[i] ?? null); }
    else if (o.textContent !== l) o.textContent = l;
    if (sel.options[i] !== o) sel.add(o, sel.options[i] ?? null);
  });
}

/** One copy of the animations per document, shared by the Animation and Timing panels and the preview. */
class MotionStore {
  data: MotionData = EMPTY;
  private busy: Promise<void> | null = null;
  private again = false;

  async refresh(): Promise<void> {
    if (this.busy) { this.again = true; return this.busy; }
    this.busy = (async () => {
      try { this.data = editor.hasDoc ? await api<MotionData>('/motion') : EMPTY; } catch { this.data = EMPTY; }
    })().finally(() => { this.busy = null; if (this.again) { this.again = false; void this.refresh(); } });
    return this.busy;
  }

  async set(ids: number[], anim: Record<string, unknown> | null): Promise<void> {
    try { await api('/motion/set', { json: { ids, anim } }); } catch (e) { toast((e as Error).message, 'error'); }
    await this.refresh();
  }

  async timing(page: number, groups: number[][]): Promise<void> {
    try { await api('/motion/timing', { json: { page, groups } }); } catch (e) { toast((e as Error).message, 'error'); }
    await this.refresh();
  }
}

/** Plays an animation on a picture of the object laid over the canvas; the canvas itself is never touched. */
class MotionPreview {
  private readonly layer = el('<div class="fan-layer" aria-hidden="true"></div>');
  private readonly ghosts = new Map<number, { root: HTMLElement; img: HTMLImageElement; rev: number; box: Rect4; spread: number }>();
  private token = 0;

  constructor(private readonly ws: Workspace, private readonly store: MotionStore) {
    ws.viewport.stage.appendChild(this.layer);
    ws.viewport.onView = ((prev) => () => { prev?.(); this.place(); })(ws.viewport.onView);
  }

  private screenRect(spread: number, b: Rect4): Rect4 | null {
    const slots = editor.layout?.slots ?? [];
    const s: any = slots.find((x: any) => (typeof x.spread === 'number' ? x.spread : x.spread?.index) === spread && (x.spread?.kind ?? 'doc') === 'doc') ?? slots[spread];
    if (!s) return null;
    const [a, , , d, e, f] = s.xf as number[];
    const [x0, y0] = this.ws.viewport.toScreen(a * b[0] + e, d * b[1] + f);
    const [x1, y1] = this.ws.viewport.toScreen(a * b[2] + e, d * b[3] + f);
    return [x0, y0, x1, y1];
  }

  private place(): void {
    for (const g of this.ghosts.values()) {
      if (g.root.hidden) continue;
      const r = this.screenRect(g.spread, g.box);
      if (!r) continue;
      const st = g.root.style;
      st.left = `${r[0]}px`; st.top = `${r[1]}px`; st.width = `${r[2] - r[0]}px`; st.height = `${r[3] - r[1]}px`;
    }
  }

  private async ghost(id: number, a: Anim): Promise<{ root: HTMLElement; img: HTMLImageElement }> {
    let g = this.ghosts.get(id);
    if (!g) {
      const root = el('<div class="fan-ghost" hidden><div class="fan-mask"></div><img class="fan-pic" alt="" draggable="false"></div>');
      this.layer.appendChild(root);
      g = { root, img: q<HTMLImageElement>(root, '.fan-pic'), rev: -1, box: a.bounds, spread: a.spread };
      this.ghosts.set(id, g);
    }
    g.box = a.bounds; g.spread = a.spread;
    if (g.rev !== docRev()) {
      await swapMedia(g.img, `/api/motion/snapshot/${id}?rev=${docRev()}&t=${Date.now()}`).catch(() => undefined);
      g.rev = docRev();
    }
    return g;
  }

  stop(): void {
    this.token++;
    for (const g of this.ghosts.values()) { for (const an of g.img.getAnimations()) an.cancel(); setHidden(g.root, true); }
  }

  async play(groups: number[][]): Promise<void> {
    this.stop();
    const mine = this.token;
    const data = this.store.data;
    for (const grp of groups) {
      if (mine !== this.token) return;
      const items = grp.map((id) => ({ id, a: data.anims[String(id)] })).filter((x) => x.a);
      const ghosts = await Promise.all(items.map((x) => this.ghost(x.id, x.a)));
      if (mine !== this.token) return;
      const runs: Animation[] = [];
      items.forEach((x, i) => {
        const g = ghosts[i];
        const o = { ...x.a.opt, iterations: x.a.opt.iterations === 'Infinity' ? 2 : x.a.opt.iterations, fill: 'both' as FillMode };
        setHidden(g.root, false);
        this.place();
        runs.push(g.img.animate(x.a.kf, o as KeyframeAnimationOptions));
      });
      await Promise.all(runs.map((r) => r.finished.catch(() => undefined)));
    }
    if (mine === this.token) await new Promise((r) => window.setTimeout(r, 350));
    if (mine === this.token) this.stop();
  }
}

export class AnimationPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel fan-panel">
    <p class="frv-hint fan-what"></p>
    <div class="fin-form fan-top">
      <label class="fin-f fin-wide"><span>Preset</span><select class="cv-select fan-preset" aria-label="Preset"></select></label>
    </div>
    <div class="fin-form fan-body" hidden>
      <label class="fin-f fin-wide"><span>Event</span><select class="cv-select fan-event" aria-label="Event"></select></label>
      <label class="fin-f fin-wide fan-btnf" hidden><span>Button</span><select class="cv-select fan-button" aria-label="Button"></select></label>
      <label class="fin-f"><span>Duration (seconds)</span><input class="fin-in fan-dur" type="number" min="0.05" max="60" step="0.1"></label>
      <label class="fin-f"><span>Delay (seconds)</span><input class="fin-in fan-delay" type="number" min="0" max="120" step="0.1"></label>
      <label class="fin-f"><span>Play (times)</span><input class="fin-in fan-repeat" type="number" min="1" max="99" step="1"></label>
      <label class="fin-f"><span>Speed</span><select class="cv-select fan-speed" aria-label="Speed"></select></label>
      <div class="fin-checks"><label class="frv-check"><input type="checkbox" class="fan-loop"><span>Loop</span></label><label class="frv-check"><input type="checkbox" class="fan-hide"><span>Hide until animated</span></label></div>
      <p class="frv-hint fin-wide">End state, empty uses the preset</p>
      <label class="fin-f"><span>Opacity (%)</span><input class="fin-in fan-opacity" type="number" min="0" max="100" step="5" placeholder="preset"></label>
      <label class="fin-f"><span>Scale (%)</span><input class="fin-in fan-scale" type="number" min="0" max="1000" step="5" placeholder="preset"></label>
      <label class="fin-f"><span>Rotate (degrees)</span><input class="fin-in fan-rotate" type="number" min="-3600" max="3600" step="15" placeholder="preset"></label>
    </div>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn fan-play" type="button" title="Play the selected animation on the page">Preview</button><button class="cv-btn frv-btn fan-playall" type="button" title="Play every animation of this page in timing order">Preview Page</button></div>
  </div>`);
  private ids: number[] = [];
  private buttons: Array<{ id: number; name: string }> = [];
  private writing = 0;

  constructor(ws: Workspace, private readonly store: MotionStore, private readonly preview: MotionPreview) {
    super(ws);
    const sel = (c: string) => q<HTMLSelectElement>(this.root, c);
    for (const c of ['.fan-preset', '.fan-event', '.fan-button', '.fan-speed']) enhanceSelect(sel(c));
    for (const c of ['.fan-dur', '.fan-delay', '.fan-repeat', '.fan-opacity', '.fan-scale', '.fan-rotate']) {
      const n = q<HTMLInputElement>(this.root, c);
      n.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') n.blur(); });
      n.addEventListener('change', () => void this.write());
    }
    for (const c of ['.fan-preset', '.fan-event', '.fan-button', '.fan-speed', '.fan-loop', '.fan-hide']) q(this.root, c).addEventListener('change', () => void this.write());
    q(this.root, '.fan-play').addEventListener('click', () => void this.play(false));
    q(this.root, '.fan-playall').addEventListener('click', () => void this.play(true));
    engine.on('motion', () => this.schedule(30));
  }

  private async play(all: boolean): Promise<void> {
    await this.store.refresh();
    const d = this.store.data;
    const groups = all ? d.timing[String(this.ws.viewport.currentPage())] ?? [] : this.ids.filter((i) => d.anims[String(i)]).map((i) => [i]);
    if (!groups.length) { toast('Nothing to play: choose a preset for the selected object first.'); return; }
    await this.preview.play(groups);
  }

  protected async load(): Promise<void> {
    const items = editor.selection?.items ?? [];
    this.ids = editor.hasDoc && !editor.selection?.text ? items : [];
    if (this.writing) return;
    await this.store.refresh();
    if (editor.hasDoc) {
      const bs = await editor.exec<Array<{ id: number; name: string }>>('button.list', {}, true).catch(() => undefined);
      this.buttons = (bs ?? []).filter((b) => !this.ids.includes(b.id));
    }
    this.render();
  }

  private render(): void {
    const d = this.store.data;
    const has = this.ids.length > 0;
    const a = has ? d.anims[String(this.ids[0])] : undefined;
    const sel = (c: string) => q<HTMLSelectElement>(this.root, c);
    patchOptions(sel('.fan-preset'), [['none', 'No Animation'], ...d.presets.map((p): [string, string] => [p.id, p.label])]);
    patchOptions(sel('.fan-event'), d.events.map((e): [string, string] => [e, EVENT_LABEL[e] ?? e]));
    patchOptions(sel('.fan-speed'), d.speeds.map((s): [string, string] => [s, SPEED_LABEL[s] ?? s]));
    patchOptions(sel('.fan-button'), this.buttons.length ? this.buttons.map((b): [string, string] => [String(b.id), b.name]) : [['', 'No buttons in this document']]);
    setText(q(this.root, '.fan-what'), !editor.hasDoc ? 'Open a document to animate its objects.' : !has ? 'Select an object to give it an animation. Animations play in the Website export, the HTML export and fixed-layout EPUB.' : this.ids.length > 1 ? `${this.ids.length} objects selected: changes apply to all of them.` : `${a ? 'Animated' : 'Not animated'}, page ${d.items[String(this.ids[0])]?.pageName ?? ''}.`);
    setDisabled(sel('.fan-preset'), !has);
    setValue(sel('.fan-preset'), a?.preset ?? 'none');
    setHidden(q(this.root, '.fan-body'), !a);
    if (a) {
      setValue(sel('.fan-event'), a.event);
      setHidden(q(this.root, '.fan-btnf'), a.event !== 'button');
      if (a.button !== undefined) setValue(sel('.fan-button'), String(a.button));
      const num = (c: string, v: number | null) => setValue(q<HTMLInputElement>(this.root, c), v === null ? '' : String(v));
      num('.fan-dur', a.duration); num('.fan-delay', a.delay); num('.fan-repeat', a.repeat);
      num('.fan-opacity', a.opacity); num('.fan-scale', a.scale); num('.fan-rotate', a.rotate);
      setValue(sel('.fan-speed'), a.speed);
      setChecked(q<HTMLInputElement>(this.root, '.fan-loop'), a.loop);
      setChecked(q<HTMLInputElement>(this.root, '.fan-hide'), a.hideBefore);
      setDisabled(q<HTMLInputElement>(this.root, '.fan-repeat'), a.loop);
    } else {
      setValue(sel('.fan-event'), 'load');
      setHidden(q(this.root, '.fan-btnf'), true);
      for (const c of ['.fan-dur', '.fan-delay', '.fan-repeat', '.fan-opacity', '.fan-scale', '.fan-rotate']) setValue(q<HTMLInputElement>(this.root, c), '');
      setValue(sel('.fan-speed'), 'preset');
      setChecked(q<HTMLInputElement>(this.root, '.fan-loop'), false);
      setDisabled(q<HTMLInputElement>(this.root, '.fan-repeat'), false);
    }
    const total = Object.keys(d.anims).length;
    setText(q(this.root, '.frv-count'), total ? `${total} animated` : 'None yet');
    setDisabled(q<HTMLButtonElement>(this.root, '.fan-play'), !a);
    setDisabled(q<HTMLButtonElement>(this.root, '.fan-playall'), !total);
    setClass(q(this.root, '.fan-btnf'), 'is-empty', !this.buttons.length);
  }

  private async write(): Promise<void> {
    if (!this.ids.length) return;
    const sel = (c: string) => q<HTMLSelectElement>(this.root, c).value;
    const inp = (c: string) => q<HTMLInputElement>(this.root, c).value.trim();
    const preset = sel('.fan-preset');
    this.writing++;
    try {
      if (preset === 'none') { await this.store.set(this.ids, null); return; }
      const optional = (c: string) => (inp(c) === '' ? null : Number(inp(c)));
      const event = sel('.fan-event') || 'load';
      const button = Number(sel('.fan-button'));
      if (event === 'button' && !(sel('.fan-button') !== '' && Number.isFinite(button))) {
        toast('There is no button in this document: make one in the Buttons panel first.');
        q<HTMLSelectElement>(this.root, '.fan-event').value = 'load';
        return;
      }
      const cur = this.store.data.anims[String(this.ids[0])];
      const presetChanged = !cur || cur.preset !== preset;
      const anim = {
        preset, event, button: event === 'button' ? button : undefined, duration: Number(inp('.fan-dur')) || cur?.duration || 1, delay: Number(inp('.fan-delay')) || 0,
        repeat: Math.max(1, Number(inp('.fan-repeat')) || 1), loop: q<HTMLInputElement>(this.root, '.fan-loop').checked, speed: sel('.fan-speed') || 'preset',
        opacity: optional('.fan-opacity'), scale: optional('.fan-scale'), rotate: optional('.fan-rotate'),
        hideBefore: presetChanged ? this.store.data.presets.find((p) => p.id === preset)?.enter ?? true : q<HTMLInputElement>(this.root, '.fan-hide').checked,
      };
      await this.store.set(this.ids, anim);
    } finally {
      this.writing--;
      this.schedule(10);
    }
  }
}

interface Row { id: number; label: string; preset: string; event: string; together: boolean; first: boolean; last: boolean }

export class TimingPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel fan-timing">
    <p class="frv-hint fan-twhat"></p>
    <div class="frv-list fan-tlist" role="list" aria-label="Animations on this page"></div>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn fan-tplay" type="button">Preview Page</button></div>
  </div>`);
  private page = 0;
  private rows: Row[] = [];

  constructor(ws: Workspace, private readonly store: MotionStore, private readonly preview: MotionPreview) {
    super(ws);
    q(this.root, '.fan-tplay').addEventListener('click', () => void this.preview.play(this.store.data.timing[String(this.page)] ?? []));
    ws.viewport.onView = ((prev) => () => { prev?.(); if (!this.visible) return; const p = ws.viewport.currentPage(); if (p !== this.page) { this.page = p; this.schedule(40); } })(ws.viewport.onView);
    engine.on('motion', () => this.schedule(30));
  }

  protected async load(): Promise<void> {
    this.page = this.ws.viewport.currentPage();
    await this.store.refresh();
    const d = this.store.data;
    const flat: Row[] = [];
    for (const g of d.timing[String(this.page)] ?? []) {
      g.forEach((id, k) => {
        const a = d.anims[String(id)];
        if (a) flat.push({ id, label: a.label, preset: d.presets.find((p) => p.id === a.preset)?.label ?? a.preset, event: EVENT_LABEL[a.event] ?? a.event, together: k > 0, first: false, last: false });
      });
    }
    flat.forEach((r, i) => { r.first = i === 0; r.last = i === flat.length - 1; });
    this.rows = flat;
    this.render();
  }

  private render(): void {
    const name = editor.layout?.pages.find((p) => p.abs === this.page)?.name ?? String(this.page + 1);
    setText(q(this.root, '.fan-twhat'), !editor.hasDoc ? 'Open a document to order its animations.' : this.rows.length ? `Page ${name}: animations play from the top down. Tick Together to start one with the one above it.` : `Page ${name} has no animations. Choose a preset in the Animation panel.`);
    patchList<Row>(q(this.root, '.fan-tlist'), this.rows, {
      key: (r) => String(r.id),
      create: (r) => {
        const arrow = (d: string) => `<svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="${d}"/></svg>`;
        const n = el(`<div class="frv-row fan-trow" role="listitem"><span class="fan-tmain"><span class="frv-text fan-tname"></span><span class="fan-tsub"></span></span><label class="frv-check fan-tog" title="Start together with the animation above"><input type="checkbox"><span>Together</span></label><span class="frv-acts"><button class="cv-btn frv-btn frv-mini fan-up" type="button" aria-label="Move up" title="Move up">${arrow('M3 10l5-5 5 5')}</button><button class="cv-btn frv-btn frv-mini fan-down" type="button" aria-label="Move down" title="Move down">${arrow('M3 6l5 5 5-5')}</button></span></div>`);
        const id = () => Number(n.dataset.key);
        q(n, '.fan-tmain').addEventListener('click', () => void editor.exec('selection.set', { ids: [id()] }, true).then(() => editor.schedule()));
        q<HTMLInputElement>(n, '.fan-tog input').addEventListener('change', (e) => this.commit(this.rows.map((r) => (r.id === id() ? { ...r, together: (e.target as HTMLInputElement).checked } : r))));
        q(n, '.fan-up').addEventListener('click', () => this.move(id(), -1));
        q(n, '.fan-down').addEventListener('click', () => this.move(id(), 1));
        this.fill(n, r);
        return n;
      },
      update: (n, r) => this.fill(n, r),
    });
    setText(q(this.root, '.frv-count'), this.rows.length ? `${this.rows.length} on this page` : '');
    setDisabled(q<HTMLButtonElement>(this.root, '.fan-tplay'), !this.rows.length);
  }

  private fill(n: HTMLElement, r: Row): void {
    setText(q(n, '.fan-tname'), r.label);
    setText(q(n, '.fan-tsub'), `${r.preset} · ${r.event}`);
    setChecked(q<HTMLInputElement>(n, '.fan-tog input'), r.together);
    setDisabled(q<HTMLInputElement>(n, '.fan-tog input'), r.first);
    setDisabled(q<HTMLButtonElement>(n, '.fan-up'), r.first);
    setDisabled(q<HTMLButtonElement>(n, '.fan-down'), r.last);
  }

  private move(id: number, by: number): void {
    const i = this.rows.findIndex((r) => r.id === id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= this.rows.length) return;
    const next = this.rows.slice();
    [next[i], next[j]] = [next[j], next[i]];
    this.commit(next);
  }

  private commit(rows: Row[]): void {
    const groups: number[][] = [];
    rows.forEach((r, i) => { if (i > 0 && r.together) groups[groups.length - 1].push(r.id); else groups.push([r.id]); });
    void this.store.timing(this.page, groups);
  }
}

export function installAnimation(ws: Workspace): void {
  const store = new MotionStore();
  let preview: MotionPreview | null = null;
  const pv = (): MotionPreview => (preview ??= new MotionPreview(ws, store));
  let anim: AnimationPanel | null = null, timing: TimingPanel | null = null;
  ws.registerPanel({ id: 'animation', label: 'Animation', group: 'interactive', column: 'side', order: 40, create: () => (anim = new AnimationPanel(ws, store, pv())).root, onShow: () => anim?.onShow() });
  ws.registerPanel({ id: 'timing', label: 'Timing', group: 'interactive', column: 'side', order: 50, create: () => (timing = new TimingPanel(ws, store, pv())).root, onShow: () => timing?.onShow() });
  ws.addCommands(
    { id: 'ui.animation', label: 'Animation', run: () => ws.showPanel('animation') },
    { id: 'ui.timing', label: 'Timing', run: () => ws.showPanel('timing') },
  );
}
