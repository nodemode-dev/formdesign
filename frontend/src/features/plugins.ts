import '../styles/plugins.css';
import { editor, friendly } from '../editor/editor';
import { api } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import type { SubMenu } from '../ui/menu';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Perms { commands?: boolean; 'document.read'?: boolean; 'document.write'?: boolean; panel?: boolean; menu?: boolean; network?: string[] }
interface Manifest {
  id: string; name: string; version: string; author: string; description: string; entry: string; minAppVersion: string; permissions: Perms;
  contributes: { commands: Array<{ id: string; label: string }>; panel: { title: string } | null; menu: string[] };
}
interface Installed extends Manifest { enabled: boolean; granted: Perms; error?: string }
interface Listing { plugins: Installed[]; catalogue: string; defaultCatalogue: string; permissions: Record<string, string>; folder: string }
interface Entry { id: string; name: string; version: string; author: string; description: string; permissions: Perms; installed: boolean; installedVersion: string | null }
interface Catalogue { url: string; name: string; plugins: Entry[] }
interface Staged { stage: string; manifest: Manifest; source: string; sha256: string; installed: boolean }
interface FrameMsg { fdp: 1; id?: number; type: string; tool?: string; args?: Record<string, unknown>; url?: string; text?: string }

/** Tool classes the broker enforces; the engine checks the same grant again and runs the call as the client plugin:<id>. */
const READ_TOOLS = new Set(['query', 'outline', 'inspect', 'look', 'get_document', 'get_slots', 'list_documents', 'preview', 'find_commands', 'preflight']);
const WRITE_TOOLS = new Set(['set_slots', 'run_commands', 'undo', 'redo', 'show']);

const BOOT = `(function(){
var pending={},seq=0,cmds={},fns=[];
function send(m){parent.postMessage(Object.assign({fdp:1},m),'*');}
function req(type,data){return new Promise(function(res,rej){var id=++seq;pending[id]={res:res,rej:rej};send(Object.assign({id:id,type:type},data));});}
function parse(r){var t=((r&&r.content)||[]).filter(function(c){return c.type==='text';}).map(function(c){return c.text;}).join('\\n');var d=null;try{d=JSON.parse(t);}catch(e){}return{isError:!!(r&&r.isError),text:t,data:d};}
window.addEventListener('message',function(e){
 if(e.source!==parent)return;var m=e.data;if(!m||m.fdp!==1)return;
 if(m.type==='reply'){var p=pending[m.id];if(!p)return;delete pending[m.id];if(m.error)p.rej(new Error(m.error));else p.res(m.result);}
 else if(m.type==='command'){var f=cmds[m.command];if(f)Promise.resolve().then(f);}
 else if(m.type==='change'){fns.forEach(function(f){try{f();}catch(x){}});}
 else if(m.type==='theme'){for(var k in m.vars)document.documentElement.style.setProperty(k,m.vars[k]);}
});
window.formdesign={
 call:function(tool,args){return req('call',{tool:tool,args:args||{}}).then(parse);},
 query:function(args){return window.formdesign.call('query',args);},
 fetch:function(url){return req('fetch',{url:url});},
 notify:function(text){return req('notify',{text:String(text)});},
 registerCommand:function(id,fn){cmds[id]=fn;},
 onChange:function(fn){fns.push(fn);}
};
send({type:'hello'});
})();`;

const THEME_VARS: Array<[string, string]> = [['--fd-bg', '--bg-primary'], ['--fd-surface', '--bg-tertiary'], ['--fd-fg', '--text-primary'], ['--fd-muted', '--text-muted'], ['--fd-line', '--border-light'], ['--fd-accent', '--accent-color'], ['--fd-font', '--font-ui']];

function describe(p: Perms, names: Record<string, string>): string[] {
  const out: string[] = [];
  for (const k of ['document.read', 'document.write', 'commands', 'panel', 'menu']) if ((p as Record<string, unknown>)[k]) out.push(names[k] ?? k);
  if (p.network?.length) out.push(`Reach these internet hosts: ${p.network.join(', ')}`);
  return out;
}

function short(p: Perms): string[] {
  const out: string[] = [];
  if (p['document.read']) out.push('Read');
  if (p['document.write']) out.push('Write');
  if (p.commands) out.push('Commands');
  if (p.panel) out.push('Panel');
  if (p.menu) out.push('Menu');
  if (p.network?.length) out.push(`Network (${p.network.length})`);
  return out;
}

function srcdoc(entry: string): string {
  const safe = entry.replace(/<\/(script)/gi, '<\\/$1');
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:"><style>:root{--fd-bg:#111;--fd-surface:#222127;--fd-fg:#f7f7f7;--fd-muted:#717072;--fd-line:#34333a;--fd-accent:#f97316;--fd-font:system-ui,sans-serif;color-scheme:dark}html,body{margin:0;background:var(--fd-bg);color:var(--fd-fg);font:12px/1.4 var(--fd-font)}</style></head><body><script>${BOOT}</script><script>${safe}</script></body></html>`;
}

interface Running { plugin: Installed; frame: HTMLIFrameElement; ready: boolean }

class PluginHost {
  readonly running = new Map<string, Running>();
  readonly submenu: SubMenu = { label: 'Plug-ins', items: [] };
  private readonly holder = el('<div class="fp-holder" aria-hidden="true"></div>');
  private readonly bodies = new Map<string, HTMLElement>();
  private readonly commandIds = new Set<string>();
  private data: Listing = { plugins: [], catalogue: '', defaultCatalogue: '', permissions: {}, folder: '' };
  private timer = 0;
  private listeners: Array<() => void> = [];

  constructor(private readonly ws: Workspace) {
    document.body.appendChild(this.holder);
    window.addEventListener('message', (e) => this.receive(e));
    const poke = () => { window.clearTimeout(this.timer); this.timer = window.setTimeout(() => this.post({ type: 'change' }, 'document.read'), 150); };
    editor.on('change', poke);
    editor.on('selection', poke);
    editor.on('docs', poke);
    this.rebuildMenu();
  }

  get listing(): Listing { return this.data; }
  onChange(fn: () => void): void { this.listeners.push(fn); }

  private chain: Promise<void> = Promise.resolve();

  /** One reload at a time, so two callers never start the same plug-in twice. */
  reload(): Promise<void> {
    const next = this.chain.then(() => this.load(), () => this.load());
    this.chain = next.catch(() => undefined);
    return next;
  }

  private async load(): Promise<void> {
    this.data = await api<Listing>('/plugins');
    const want = new Map(this.data.plugins.filter((p) => p.enabled && !p.error).map((p) => [p.id, p]));
    for (const [id, r] of [...this.running]) {
      const w = want.get(id);
      if (!w || w.version !== r.plugin.version) this.stop(id);
    }
    for (const p of want.values()) if (!this.running.has(p.id)) await this.start(p);
    this.rebuildMenu();
    for (const fn of this.listeners) fn();
  }

  private panelBody(p: Installed): HTMLElement {
    let body = this.bodies.get(p.id);
    if (!body) {
      body = el('<div class="fp-panel"><p class="fp-off">This plug-in is turned off. Turn it on in Window > Plug-ins.</p></div>');
      this.bodies.set(p.id, body);
      this.ws.registerPanel({ id: `plugin.${p.id}`, label: p.contributes.panel?.title ?? p.name, group: 'plugins', column: 'side', order: 70, create: () => body as HTMLElement });
      if (!this.ws.commands.get(`ui.panel.plugin.${p.id}`)) this.ws.addCommands({ id: `ui.panel.plugin.${p.id}`, label: p.contributes.panel?.title ?? p.name, run: () => this.ws.showPanel(`plugin.${p.id}`) });
    }
    return body;
  }

  private async start(p: Installed): Promise<void> {
    let source: string;
    try { const r = await fetch(`/api/plugins/${encodeURIComponent(p.id)}/entry`); if (!r.ok) throw new Error(r.statusText); source = await r.text(); } catch (err) { toast(`${p.name} could not start: ${friendly((err as Error).message)}`, 'error'); return; }
    const frame = el<HTMLIFrameElement>('<iframe class="fp-frame" title="" referrerpolicy="no-referrer"></iframe>');
    frame.setAttribute('sandbox', 'allow-scripts');
    frame.title = p.name;
    const run: Running = { plugin: p, frame, ready: false };
    this.running.set(p.id, run);
    const wantsPanel = !!p.granted.panel && !!p.contributes.panel;
    if (wantsPanel) { const body = this.panelBody(p); q(body, '.fp-off').hidden = true; body.appendChild(frame); } else this.holder.appendChild(frame);
    if (p.granted.commands) for (const c of p.contributes.commands) this.command(p, c.id, c.label);
    frame.srcdoc = srcdoc(source);
  }

  private stop(id: string): void {
    const r = this.running.get(id);
    if (!r) return;
    r.frame.remove();
    this.running.delete(id);
    const body = this.bodies.get(id);
    if (body) q(body, '.fp-off').hidden = false;
  }

  private command(p: Installed, cid: string, label: string): void {
    const id = `plugin.${p.id}.${cid}`;
    if (this.commandIds.has(id)) return;
    this.commandIds.add(id);
    this.ws.addCommands({ id, label, enabled: () => !!this.running.get(p.id)?.ready, run: () => this.running.get(p.id)?.frame.contentWindow?.postMessage({ fdp: 1, type: 'command', command: cid }, '*') });
  }

  private rebuildMenu(): void {
    const items: SubMenu['items'] = ['ui.plugins'];
    const live = this.data.plugins.filter((p) => p.enabled && !p.error && p.granted.menu && p.granted.commands && p.contributes.menu.length);
    if (live.length) items.push('-');
    for (const p of live) for (const m of p.contributes.menu) items.push(`plugin.${p.id}.${m}`);
    this.submenu.items.splice(0, this.submenu.items.length, ...items);
  }

  private post(msg: Record<string, unknown>, need?: keyof Perms): void {
    for (const r of this.running.values()) if (r.ready && (!need || r.plugin.granted[need])) r.frame.contentWindow?.postMessage({ fdp: 1, ...msg }, '*');
  }

  private receive(e: MessageEvent): void {
    const m = e.data as FrameMsg | undefined;
    if (!m || m.fdp !== 1) return;
    const run = [...this.running.values()].find((r) => r.frame.contentWindow === e.source);
    if (!run) return;
    const reply = (result?: unknown, error?: string) => run.frame.contentWindow?.postMessage({ fdp: 1, type: 'reply', id: m.id, result, error }, '*');
    if (m.type === 'hello') {
      run.ready = true;
      const cs = getComputedStyle(document.documentElement);
      const vars: Record<string, string> = {};
      for (const [to, from] of THEME_VARS) { const v = cs.getPropertyValue(from).trim(); if (v) vars[to] = v; }
      run.frame.contentWindow?.postMessage({ fdp: 1, type: 'theme', vars }, '*');
      return;
    }
    void this.handle(run, m, reply);
  }

  private async handle(run: Running, m: FrameMsg, reply: (r?: unknown, e?: string) => void): Promise<void> {
    const p = run.plugin;
    const refuse = (what: string) => { toast(`${p.name} was refused: ${what}`, 'error'); reply(undefined, `permission denied: ${what}`); };
    try {
      if (m.type === 'call') {
        const tool = String(m.tool ?? '');
        const need = READ_TOOLS.has(tool) ? 'document.read' : WRITE_TOOLS.has(tool) ? 'document.write' : null;
        if (!need) return refuse(`the ${tool} tool is not available to plug-ins`);
        if (!p.granted[need]) return refuse(`${need} was not granted`);
        const r = await api<unknown>(`/plugins/${encodeURIComponent(p.id)}/call`, { json: { tool, args: m.args ?? {} } });
        editor.schedule();
        return reply(r);
      }
      if (m.type === 'fetch') {
        let host = '';
        try { host = new URL(String(m.url)).hostname.toLowerCase(); } catch { /* refused below */ }
        if (!host || !p.granted.network?.includes(host)) return refuse(`network access to ${host || String(m.url)} was not granted`);
        const r = await api<{ text: string }>(`/plugins/${encodeURIComponent(p.id)}/fetch`, { json: { url: m.url } });
        return reply(r.text);
      }
      if (m.type === 'notify') { toast(`${p.name}: ${String(m.text ?? '').slice(0, 200)}`); return reply(true); }
      reply(undefined, `unknown request ${m.type}`);
    } catch (err) { reply(undefined, friendly((err as Error).message)); }
  }
}

class Consent {
  private readonly d = new Dialog('Install Plug-in', 460);
  private readonly body = el('<div class="fp-consent"><div class="fp-chead"><span class="fp-cname"></span><span class="fp-cver"></span></div><p class="fp-cby"></p><p class="fp-cdesc"></p><p class="fp-cask">This plug-in asks to:</p><ul class="fp-cperms"></ul><p class="fp-cnote"></p></div>');
  private done: ((ok: boolean) => void) | null = null;
  private readonly ok: HTMLButtonElement;

  constructor() {
    this.d.body.append(this.body);
    this.d.button('Cancel', 'ghost', () => this.finish(false));
    this.ok = this.d.button('Allow and Install', 'primary', () => this.finish(true));
    this.d.onClose = () => { this.done?.(false); this.done = null; };
  }

  private finish(v: boolean): void { const f = this.done; this.done = null; this.d.close(); f?.(v); }

  ask(s: Staged, names: Record<string, string>): Promise<boolean> {
    const m = s.manifest;
    setText(q(this.body, '.fp-cname'), m.name);
    setText(q(this.body, '.fp-cver'), `v${m.version}`);
    setText(q(this.body, '.fp-cby'), `by ${m.author}`);
    setText(q(this.body, '.fp-cdesc'), m.description);
    const perms = describe(m.permissions, names);
    patchList<string>(q(this.body, '.fp-cperms'), perms.length ? perms : ['Nothing: it asks for no permissions'], { key: (t) => t, create: (t) => { const li = document.createElement('li'); li.textContent = t; return li; }, update: (li, t) => { if (li.textContent !== t) li.textContent = t; } });
    setText(q(this.body, '.fp-cnote'), `It runs in a sandbox and can only use what is listed. Its changes to the document run as the agent client plugin:${m.id}, so Window > Agent Connect can set it to Ask or Deny.${s.installed ? ' This replaces the version already installed.' : ''}`);
    setText(this.ok, s.installed ? 'Allow and Update' : 'Allow and Install');
    return new Promise((res) => { this.done = res; this.d.open(this.ok); });
  }
}

class Manager {
  private readonly d = new Dialog('Plug-ins', 640);
  private readonly root = el(`<div class="fp">
    <div class="fp-tabs" role="tablist"><button class="fp-tab is-on" type="button" role="tab" data-tab="installed" aria-selected="true">Installed</button><button class="fp-tab" type="button" role="tab" data-tab="browse" aria-selected="false">Browse</button></div>
    <section class="fp-pane" data-pane="installed">
      <div class="fp-list fp-installed" role="list" aria-label="Installed plug-ins"></div>
      <p class="fp-empty fp-none" hidden>No plug-ins installed. Browse the catalogue, or install one from a folder or a .fdplugin file.</p>
    </section>
    <section class="fp-pane" data-pane="browse" hidden>
      <div class="fp-urlbar"><input class="cv-input fp-url" type="text" spellcheck="false" aria-label="Catalogue address"><button class="cv-btn fp-save" type="button">Use</button><button class="cv-btn fp-reload" type="button">Refresh</button></div>
      <div class="fp-list fp-browse" role="list" aria-label="Catalogue"></div>
      <p class="fp-empty fp-bnone" hidden></p>
    </section>
    <p class="fp-status" role="status"></p>
  </div>`);
  private readonly installed = q(this.root, '.fp-installed');
  private readonly browse = q(this.root, '.fp-browse');
  private cat: Catalogue | null = null;
  private busy = new Set<string>();

  constructor(private readonly ws: Workspace, private readonly host: PluginHost, private readonly consent: Consent) {
    this.d.body.append(this.root);
    this.d.button('Install from File…', 'ghost', () => void this.fromFile());
    this.d.button('Close', 'primary', () => this.d.close());
    for (const t of Array.from(this.root.querySelectorAll<HTMLElement>('.fp-tab'))) t.addEventListener('click', () => this.tab(t.dataset.tab as string));
    q(this.root, '.fp-save').addEventListener('click', () => void this.saveUrl());
    q(this.root, '.fp-reload').addEventListener('click', () => void this.loadCatalogue());
    q(this.root, '.fp-url').addEventListener('keydown', (e) => { e.stopPropagation(); if ((e as KeyboardEvent).key === 'Enter') void this.saveUrl(); });
    this.installed.addEventListener('click', (e) => void this.clickInstalled(e));
    this.browse.addEventListener('click', (e) => void this.clickBrowse(e));
    host.onChange(() => { if (this.d.isOpen) this.paint(); });
  }

  async open(): Promise<void> {
    this.d.open();
    await this.host.reload().catch((err) => this.say(friendly((err as Error).message)));
    this.paint();
  }

  private say(t: string): void { setText(q(this.root, '.fp-status'), t); }

  private tab(name: string): void {
    for (const t of Array.from(this.root.querySelectorAll<HTMLElement>('.fp-tab'))) { const on = t.dataset.tab === name; setClass(t, 'is-on', on); t.setAttribute('aria-selected', String(on)); }
    for (const s of Array.from(this.root.querySelectorAll<HTMLElement>('.fp-pane'))) setHidden(s, s.dataset.pane !== name);
    if (name === 'browse' && !this.cat) void this.loadCatalogue();
  }

  private async loadCatalogue(): Promise<void> {
    this.say('Reading the catalogue…');
    try { this.cat = await api<Catalogue>('/plugins/catalogue'); this.say(''); } catch (err) { this.cat = null; this.say(friendly((err as Error).message)); }
    this.paint();
  }

  private async saveUrl(): Promise<void> {
    const v = q<HTMLInputElement>(this.root, '.fp-url').value.trim();
    try { await api('/plugins/catalogue/url', { json: { url: v } }); await this.host.reload(); await this.loadCatalogue(); } catch (err) { this.say(friendly((err as Error).message)); }
  }

  private paint(): void {
    const l = this.host.listing;
    const url = q<HTMLInputElement>(this.root, '.fp-url');
    if (document.activeElement !== url && url.value !== l.catalogue) url.value = l.catalogue;
    setHidden(q(this.root, '.fp-none'), l.plugins.length > 0);
    patchList<Installed>(this.installed, l.plugins, {
      key: (p) => p.id,
      create: (p) => {
        const r = el(`<div class="fp-row" role="listitem"><div class="fp-main"><span class="fp-name"></span><span class="fp-ver"></span><span class="fp-by"></span></div><p class="fp-desc"></p><div class="fp-chips"></div><div class="fp-acts"><button class="cv-btn fp-toggle" type="button"></button><button class="cv-btn cv-btn-danger fp-rm" type="button">Uninstall</button></div></div>`);
        this.fillInstalled(r, p);
        return r;
      },
      update: (r, p) => this.fillInstalled(r, p),
    });
    const cat = this.cat;
    const none = q(this.root, '.fp-bnone');
    setHidden(none, !!cat && cat.plugins.length > 0);
    if (!cat) setText(none, 'The catalogue has not been read.');
    else if (!cat.plugins.length) setText(none, 'This catalogue lists no plug-ins.');
    patchList<Entry>(this.browse, cat?.plugins ?? [], {
      key: (p) => p.id,
      create: (p) => {
        const r = el(`<div class="fp-row" role="listitem"><div class="fp-main"><span class="fp-name"></span><span class="fp-ver"></span><span class="fp-by"></span></div><p class="fp-desc"></p><div class="fp-chips"></div><div class="fp-acts"><button class="cv-btn cv-btn-primary fp-get" type="button"></button></div></div>`);
        this.fillEntry(r, p);
        return r;
      },
      update: (r, p) => this.fillEntry(r, p),
    });
  }

  private fillCommon(r: HTMLElement, p: { name: string; version: string; author: string; description: string }, perms: Perms): void {
    setText(q(r, '.fp-name'), p.name);
    setText(q(r, '.fp-ver'), `v${p.version}`);
    setText(q(r, '.fp-by'), `by ${p.author}`);
    setText(q(r, '.fp-desc'), p.description);
    const chips = q(r, '.fp-chips');
    patchList<string>(chips, short(perms), { key: (c) => c, create: (c) => { const s = document.createElement('span'); s.className = 'fp-chip'; s.textContent = c; return s; }, update: () => undefined });
  }

  private fillInstalled(r: HTMLElement, p: Installed): void {
    this.fillCommon(r, p, p.granted);
    if (p.error) setText(q(r, '.fp-desc'), p.error);
    setText(q(r, '.fp-toggle'), p.enabled ? 'Disable' : 'Enable');
    setClass(r, 'is-off', !p.enabled);
    setDisabled(q<HTMLButtonElement>(r, '.fp-toggle'), !!p.error || this.busy.has(p.id));
    setDisabled(q<HTMLButtonElement>(r, '.fp-rm'), this.busy.has(p.id));
  }

  private fillEntry(r: HTMLElement, p: Entry): void {
    this.fillCommon(r, p, p.permissions);
    const b = q<HTMLButtonElement>(r, '.fp-get');
    const upd = p.installed && p.installedVersion !== p.version;
    setText(b, this.busy.has(p.id) ? 'Installing…' : p.installed ? (upd ? 'Update' : 'Installed') : 'Install');
    setDisabled(b, this.busy.has(p.id) || (p.installed && !upd));
  }

  private async clickInstalled(e: MouseEvent): Promise<void> {
    const t = e.target as HTMLElement;
    const row = t.closest<HTMLElement>('[data-key]');
    const p = row ? this.host.listing.plugins.find((x) => x.id === row.dataset.key) : undefined;
    if (!row || !p) return;
    this.busy.add(p.id);
    this.paint();
    try {
      if (t.closest('.fp-toggle')) await api(`/plugins/${encodeURIComponent(p.id)}/enabled`, { json: { enabled: !p.enabled } });
      else if (t.closest('.fp-rm')) { await api(`/plugins/${encodeURIComponent(p.id)}/uninstall`, { method: 'POST' }); toast(`${p.name} uninstalled`); }
      await this.host.reload();
      if (this.cat) await this.loadCatalogue();
    } catch (err) { this.say(friendly((err as Error).message)); }
    this.busy.delete(p.id);
    this.paint();
  }

  private async clickBrowse(e: MouseEvent): Promise<void> {
    const row = (e.target as HTMLElement).closest<HTMLElement>('[data-key]');
    if (!row || !(e.target as HTMLElement).closest('.fp-get')) return;
    const id = row.dataset.key as string;
    this.busy.add(id);
    this.paint();
    try {
      const s = await api<Staged>('/plugins/stage', { json: { catalogue: id } });
      await this.finishInstall(s);
    } catch (err) { this.say(friendly((err as Error).message)); }
    this.busy.delete(id);
    this.paint();
  }

  private async finishInstall(s: Staged): Promise<void> {
    if (!(await this.consent.ask(s, this.host.listing.permissions))) { this.say('Not installed.'); return; }
    await api('/plugins/install', { json: { stage: s.stage, consent: true } });
    await this.host.reload();
    if (this.cat) await this.loadCatalogue();
    this.say(`${s.manifest.name} installed.`);
    toast(`${s.manifest.name} installed`);
  }

  private async fromFile(): Promise<void> {
    const files = await this.ws.pickFiles('.fdplugin,.zip');
    if (!files.length) return;
    try {
      const [path] = await this.ws.upload(files);
      await this.finishInstall(await api<Staged>('/plugins/stage', { json: { path } }));
    } catch (err) { this.say(friendly((err as Error).message)); }
    this.paint();
  }
}

export function installPlugins(ws: Workspace): void {
  const host = new PluginHost(ws);
  const consent = new Consent();
  let manager: Manager | null = null;
  ws.addCommands({ id: 'ui.plugins', label: 'Plug-ins…', run: () => void (manager ??= new Manager(ws, host, consent)).open() });
  ws.addMenuItems({ menu: 'Window', items: [host.submenu], at: 'end' });
  editor.on('ready', () => void host.reload().catch(() => undefined));
  void host.reload().catch(() => undefined);
}
