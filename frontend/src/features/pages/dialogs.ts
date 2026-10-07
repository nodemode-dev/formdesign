import { Dialog } from '../../ui/dialogs';
import { el, q, setClass, setText } from '../../ui/dom';
import { enhanceSelect } from '../../ui/dropdown';

export type FieldKind = 'number' | 'text' | 'select' | 'check';
export interface FieldSpec { key: string; label: string; kind: FieldKind; min?: number; max?: number; step?: number; placeholder?: string; note?: string }
export type FormValues = Record<string, string | boolean>;
export interface AskOptions { values?: FormValues; options?: Record<string, Array<[string, string]>>; note?: string; extra?: { label: string; value: FormValues } }

export class FormDialog {
  private readonly d: Dialog;
  private readonly inputs = new Map<string, HTMLInputElement | HTMLSelectElement>();
  private readonly rows = new Map<string, HTMLElement>();
  private readonly noteEl: HTMLElement;
  private readonly ok: HTMLButtonElement;
  private readonly extraBtn: HTMLButtonElement;
  private extraValue: FormValues | null = null;
  private resolve: ((v: FormValues | null) => void) | null = null;
  onInput: ((values: FormValues, dlg: FormDialog) => void) | null = null;

  constructor(title: string, fields: FieldSpec[], okLabel: string, width = 420) {
    this.d = new Dialog(title, width);
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('pgd-dialog');
    for (const f of fields) {
      let input: HTMLInputElement | HTMLSelectElement;
      if (f.kind === 'select') input = el<HTMLSelectElement>('<select class="cv-input pgd-select"></select>');
      else if (f.kind === 'check') input = el<HTMLInputElement>('<input type="checkbox">');
      else input = el<HTMLInputElement>(`<input class="cv-input pgd-input" type="${f.kind === 'number' ? 'number' : 'text'}" spellcheck="false" autocomplete="off">`);
      if (input instanceof HTMLInputElement && f.kind === 'number') {
        if (f.min !== undefined) input.min = String(f.min);
        if (f.max !== undefined) input.max = String(f.max);
        input.step = String(f.step ?? 1);
        input.classList.add('pgd-num');
      }
      if (f.placeholder && input instanceof HTMLInputElement) input.placeholder = f.placeholder;
      input.setAttribute('aria-label', f.label);
      const row = f.kind === 'check'
        ? el('<label class="cv-field pgd-check"><span></span><span class="pgd-check-box"></span></label>')
        : this.d.field(f.label, input);
      if (f.kind === 'check') {
        setText(row.querySelector('span') as HTMLElement, '');
        const box = q(row, '.pgd-check-box');
        box.append(input, el('<span class="pgd-check-label"></span>'));
        setText(q(box, '.pgd-check-label'), f.label);
        this.d.body.appendChild(row);
      }
      if (f.kind === 'select') enhanceSelect(input as HTMLSelectElement);
      input.addEventListener('input', () => this.onInput?.(this.values(), this));
      input.addEventListener('change', () => this.onInput?.(this.values(), this));
      if (input instanceof HTMLInputElement && f.kind !== 'check') input.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); this.ok.click(); } });
      this.inputs.set(f.key, input);
      this.rows.set(f.key, row);
    }
    this.noteEl = el('<p class="cv-note pgd-note" hidden></p>');
    this.d.body.appendChild(this.noteEl);
    this.extraBtn = this.d.button('', 'ghost', () => { const v = this.extraValue; this.finish(v ? { ...this.values(), ...v } : null); });
    this.extraBtn.classList.add('cv-btn-apart');
    this.d.button('Cancel', 'ghost', () => this.finish(null));
    this.ok = this.d.button(okLabel, 'primary', () => this.finish(this.values()));
    this.d.onClose = () => this.finish(null);
  }

  values(): FormValues {
    const out: FormValues = {};
    for (const [k, inp] of this.inputs) out[k] = inp instanceof HTMLInputElement && inp.type === 'checkbox' ? inp.checked : inp.value;
    return out;
  }

  setOff(key: string, off: boolean): void {
    const row = this.rows.get(key), inp = this.inputs.get(key);
    if (row) setClass(row, 'is-off', off);
    if (inp && inp.disabled !== off) inp.disabled = off;
  }

  setNote(text: string): void { setText(this.noteEl, text); this.noteEl.hidden = !text; }

  ask(opts: AskOptions = {}): Promise<FormValues | null> {
    for (const [k, list] of Object.entries(opts.options ?? {})) {
      const sel = this.inputs.get(k);
      if (!(sel instanceof HTMLSelectElement)) continue;
      const key = JSON.stringify(list);
      if (sel.dataset.opts !== key) {
        sel.dataset.opts = key;
        sel.replaceChildren(...list.map(([v, label]) => { const o = document.createElement('option'); o.value = v; o.textContent = label; return o; }));
      }
    }
    for (const [k, v] of Object.entries(opts.values ?? {})) {
      const inp = this.inputs.get(k);
      if (!inp) continue;
      if (inp instanceof HTMLInputElement && inp.type === 'checkbox') inp.checked = !!v;
      else inp.value = String(v);
    }
    this.setNote(opts.note ?? '');
    this.extraValue = opts.extra?.value ?? null;
    setText(this.extraBtn, opts.extra?.label ?? '');
    this.extraBtn.hidden = !opts.extra;
    this.onInput?.(this.values(), this);
    const first = [...this.inputs.values()].find((i) => !i.disabled && !(i instanceof HTMLSelectElement) && (i as HTMLInputElement).type !== 'checkbox') as HTMLInputElement | undefined;
    this.d.open(first ?? this.ok);
    first?.select();
    return new Promise((r) => { this.resolve = r; });
  }

  private finish(v: FormValues | null): void {
    const r = this.resolve;
    this.resolve = null;
    if (this.d.isOpen) this.d.close();
    r?.(v);
  }
}

const cache = new Map<string, FormDialog>();
export function form(id: string, make: () => FormDialog): FormDialog {
  let d = cache.get(id);
  if (!d) { d = make(); cache.set(id, d); }
  return d;
}

export const WHERE: Array<[string, string]> = [['after', 'After Page'], ['before', 'Before Page'], ['start', 'At Start of Document'], ['end', 'At End of Document']];

export const LAYER_COLORS: Array<[string, number[]]> = [
  ['Light Blue', [79, 153, 255]], ['Red', [255, 59, 48]], ['Green', [79, 255, 79]], ['Blue', [43, 71, 255]], ['Yellow', [255, 210, 0]],
  ['Magenta', [255, 79, 255]], ['Cyan', [79, 230, 255]], ['Gray', [128, 128, 128]], ['Orange', [255, 128, 0]], ['Dark Green', [0, 128, 0]],
  ['Teal', [0, 128, 128]], ['Tan', [210, 180, 140]], ['Brown', [153, 51, 0]], ['Violet', [153, 51, 255]], ['Gold', [255, 153, 0]],
  ['Dark Blue', [0, 0, 135]], ['Pink', [255, 153, 204]], ['Lavender', [153, 153, 255]], ['Grass Green', [153, 204, 0]], ['Black', [0, 0, 0]],
];

export function colorName(c: number[] | undefined): string {
  if (!c) return 'Light Blue';
  let best = LAYER_COLORS[0][0], bd = Infinity;
  for (const [name, v] of LAYER_COLORS) {
    const d = (v[0] - c[0]) ** 2 + (v[1] - c[1]) ** 2 + (v[2] - c[2]) ** 2;
    if (d < bd) { bd = d; best = name; }
  }
  return bd < 900 ? best : 'Custom';
}
