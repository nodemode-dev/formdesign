import { api } from '../engine/client';
import { el, q, setChecked, setClass, setText } from '../ui/dom';
import { enhanceSelect } from '../ui/dropdown';
import { toast } from '../ui/toast';

export interface AiSettings {
  enabled: boolean; provider: string; model: string; baseUrl: string; apiKeySet: boolean; configured: boolean; imageConfigured: boolean;
  defaultUrls: Record<string, string>;
  image: { provider: string; model: string; baseUrl: string; apiKeySet: boolean; defaultUrls: Record<string, string> };
}

export const AI_CHANGED = 'fd:ai-changed';
export const loadAi = (): Promise<AiSettings> => api<AiSettings>('/assistant/settings');

const MODEL_HINT: Record<string, string> = { anthropic: 'claude-sonnet-4-5', openai: 'gpt-4.1', ollama: 'llama3.2-vision' };
const IMAGE_HINT: Record<string, string> = { openai: 'gpt-image-1', stability: 'core' };

/** Preferences > AI: one page, built once. Keys are typed here and sent once; the engine keeps them and only says whether one is set. */
export class AiPage {
  readonly root = el(`<div class="fd-pr-ai">
    <section class="cv-prefs-group">
      <h3 class="cv-prefs-label">Assistant and AI helpers</h3>
      <p class="cv-note">Off by default. When on, the Assistant panel, Generate Alt Text, Auto Style (model path), Generate Image and Generative Expand send text or pictures to the provider you choose below. Nothing is sent while this is off.</p>
      <label class="cv-opt"><input class="fa-on" type="checkbox"><span>Turn on AI features</span></label>
    </section>
    <section class="cv-prefs-group">
      <h3 class="cv-prefs-label">Language model</h3>
      <p class="cv-note">Anthropic uses the Messages API with tool use. OpenAI compatible covers OpenAI, OpenRouter, LM Studio and vLLM (set the base URL). Ollama runs on this computer. Alt text needs a model that can read images.</p>
      <label class="fa-row"><span>Provider</span><select class="cv-select fa-provider" aria-label="Provider"><option value="anthropic">Anthropic</option><option value="openai">OpenAI compatible</option><option value="ollama">Ollama (local)</option></select></label>
      <label class="fa-row"><span>Model</span><input class="cv-input fa-model" type="text" autocomplete="off" spellcheck="false"></label>
      <label class="fa-row"><span>Base URL</span><input class="cv-input fa-url" type="text" autocomplete="off" spellcheck="false"></label>
      <label class="fa-row"><span>API key</span><span class="fa-keywrap"><input class="cv-input fa-key" type="password" autocomplete="new-password" spellcheck="false"><span class="fa-keystate"></span><button class="cv-btn fa-keyclear" type="button">Remove</button></span></label>
    </section>
    <section class="cv-prefs-group">
      <h3 class="cv-prefs-label">Images</h3>
      <p class="cv-note">For Generate Image and Generative Expand. An OpenAI compatible images endpoint (generations and edits with a mask) or a Stability compatible one. A blank key and address here use the language model's when it is OpenAI compatible.</p>
      <label class="fa-row"><span>Provider</span><select class="cv-select fa-iprovider" aria-label="Image provider"><option value="openai">OpenAI compatible</option><option value="stability">Stability compatible</option></select></label>
      <label class="fa-row"><span>Model</span><input class="cv-input fa-imodel" type="text" autocomplete="off" spellcheck="false"></label>
      <label class="fa-row"><span>Base URL</span><input class="cv-input fa-iurl" type="text" autocomplete="off" spellcheck="false"></label>
      <label class="fa-row"><span>API key</span><span class="fa-keywrap"><input class="cv-input fa-ikey" type="password" autocomplete="new-password" spellcheck="false"><span class="fa-ikeystate"></span><button class="cv-btn fa-ikeyclear" type="button">Remove</button></span></label>
    </section>
    <section class="cv-prefs-group">
      <div class="fa-actions"><button class="cv-btn cv-btn-primary fa-save" type="button">Save</button><button class="cv-btn fa-test" type="button">Test the model</button><span class="fa-result"></span></div>
    </section>
  </div>`);
  private cur: AiSettings | null = null;
  private readonly on = q<HTMLInputElement>(this.root, '.fa-on');

  constructor() {
    for (const s of Array.from(this.root.querySelectorAll<HTMLSelectElement>('select'))) enhanceSelect(s);
    this.on.addEventListener('change', () => void this.save({ enabled: this.on.checked }, false));
    q(this.root, '.fa-provider').addEventListener('change', () => this.hints());
    q(this.root, '.fa-iprovider').addEventListener('change', () => this.hints());
    q(this.root, '.fa-save').addEventListener('click', () => void this.save(this.form(), true));
    q(this.root, '.fa-keyclear').addEventListener('click', () => void this.save({ clearKey: true }, true));
    q(this.root, '.fa-ikeyclear').addEventListener('click', () => void this.save({ image: { clearKey: true } }, true));
    q(this.root, '.fa-test').addEventListener('click', () => void this.test());
  }

  private f(sel: string): HTMLInputElement { return q<HTMLInputElement>(this.root, sel); }

  private form(): Record<string, unknown> {
    const key = this.f('.fa-key').value.trim(), ikey = this.f('.fa-ikey').value.trim();
    return {
      enabled: this.on.checked, provider: q<HTMLSelectElement>(this.root, '.fa-provider').value, model: this.f('.fa-model').value, baseUrl: this.f('.fa-url').value,
      ...(key ? { apiKey: key } : {}),
      image: { provider: q<HTMLSelectElement>(this.root, '.fa-iprovider').value, model: this.f('.fa-imodel').value, baseUrl: this.f('.fa-iurl').value, ...(ikey ? { apiKey: ikey } : {}) },
    };
  }

  private hints(): void {
    const c = this.cur;
    const p = q<HTMLSelectElement>(this.root, '.fa-provider').value, ip = q<HTMLSelectElement>(this.root, '.fa-iprovider').value;
    this.f('.fa-model').placeholder = MODEL_HINT[p] ?? '';
    this.f('.fa-url').placeholder = c?.defaultUrls[p] ?? '';
    this.f('.fa-imodel').placeholder = IMAGE_HINT[ip] ?? '';
    this.f('.fa-iurl').placeholder = c?.image.defaultUrls[ip] ?? '';
  }

  async load(): Promise<void> {
    try { this.cur = await loadAi(); } catch (e) { setText(q(this.root, '.fa-result'), (e as Error).message); return; }
    const c = this.cur;
    setChecked(this.on, c.enabled);
    const sync = (sel: string, v: string): void => { const n = this.f(sel); if (n.value !== v && document.activeElement !== n) n.value = v; };
    const selv = (sel: string, v: string): void => { const n = q<HTMLSelectElement>(this.root, sel); if (n.value !== v) { n.value = v; n.dispatchEvent(new Event('change')); } };
    selv('.fa-provider', c.provider);
    selv('.fa-iprovider', c.image.provider);
    sync('.fa-model', c.model); sync('.fa-url', c.baseUrl); sync('.fa-imodel', c.image.model); sync('.fa-iurl', c.image.baseUrl);
    this.keyState('.fa-keystate', '.fa-keyclear', '.fa-key', c.apiKeySet);
    this.keyState('.fa-ikeystate', '.fa-ikeyclear', '.fa-ikey', c.image.apiKeySet);
    this.hints();
  }

  private keyState(stateSel: string, clearSel: string, inputSel: string, set: boolean): void {
    setText(q(this.root, stateSel), set ? 'Set' : 'Not set');
    setClass(q(this.root, stateSel), 'is-set', set);
    (q(this.root, clearSel) as HTMLButtonElement).hidden = !set;
    this.f(inputSel).placeholder = set ? 'Set. Type a new key to replace it' : 'Paste the key';
  }

  private async save(body: Record<string, unknown>, say: boolean): Promise<void> {
    try {
      await api('/assistant/settings', { json: body });
      this.f('.fa-key').value = '';
      this.f('.fa-ikey').value = '';
      await this.load();
      document.dispatchEvent(new CustomEvent(AI_CHANGED));
      if (say) toast('AI settings saved');
    } catch (e) { toast((e as Error).message, 'error'); await this.load(); }
  }

  private async test(): Promise<void> {
    const out = q(this.root, '.fa-result');
    setText(out, 'Asking the model...');
    try {
      await api('/assistant/settings', { json: this.form() });
      this.f('.fa-key').value = '';
      this.f('.fa-ikey').value = '';
      await this.load();
      const r = await api<{ reply: string }>('/assistant/test', { json: {} });
      setText(out, `The model answered: ${r.reply}`);
      document.dispatchEvent(new CustomEvent(AI_CHANGED));
    } catch (e) { setText(out, (e as Error).message); }
  }
}
