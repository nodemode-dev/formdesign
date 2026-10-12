import { engine } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { el, q } from '../ui/dom';
import { icon } from '../ui/icons';
import type { Workspace } from '../ui/workspace';

interface Settings { on: boolean; capitalize: boolean; pairs: Record<string, string> }

const KEY = 'formdes.autocorrect';
const DEFAULT_PAIRS: Record<string, string> = {
  teh: 'the', adn: 'and', taht: 'that', thier: 'their', recieve: 'receive', recieved: 'received', seperate: 'separate', definately: 'definitely',
  occured: 'occurred', occurence: 'occurrence', accomodate: 'accommodate', acheive: 'achieve', beleive: 'believe', wich: 'which', becuase: 'because',
  goverment: 'government', enviroment: 'environment', untill: 'until', tommorow: 'tomorrow', wierd: 'weird', alot: 'a lot', dont: "don't", doesnt: "doesn't",
  isnt: "isn't", didnt: "didn't", im: "I'm", ive: "I've", youre: "you're", thats: "that's", hte: 'the', fo: 'of', ot: 'to',
  publically: 'publicly', neccessary: 'necessary', occassion: 'occasion', persue: 'pursue', posession: 'possession', reccomend: 'recommend', tounge: 'tongue',
};

function load(): Settings {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Partial<Settings> | null;
    if (s && typeof s === 'object') return { on: s.on === true, capitalize: s.capitalize === true, pairs: s.pairs && typeof s.pairs === 'object' ? s.pairs : { ...DEFAULT_PAIRS } };
  } catch { }
  return { on: false, capitalize: false, pairs: { ...DEFAULT_PAIRS } };
}

function save(s: Settings): void {
  try { localStorage.setItem(KEY, JSON.stringify(s)); } catch { }
}

async function push(s: Settings): Promise<void> {
  try { await engine.call('exec', { command: 'edit.autocorrect', params: { on: s.on, capitalize: s.capitalize, pairs: s.pairs } }); } catch { }
}

function open(state: { s: Settings }): void {
  const d = new Dialog('Autocorrect', 520);
  const body = el(`<div class="fac">
    <label class="fae-check"><input type="checkbox" class="fac-on"> Enable Autocorrect</label>
    <label class="fae-check"><input type="checkbox" class="fac-caps"> Capitalise the first word of each sentence</label>
    <p class="cv-note">A misspelling is replaced as you type the space or punctuation after it. Capitalised and all-capital words keep their case.</p>
    <div class="fac-rows"></div>
    <button class="cv-btn fac-add" type="button">${icon('add', 14)}<span>Add</span></button>
  </div>`);
  q<HTMLInputElement>(body, '.fac-on').checked = state.s.on;
  q<HTMLInputElement>(body, '.fac-caps').checked = state.s.capitalize;
  const rows = q(body, '.fac-rows');
  const add = (from: string, to: string) => {
    const r = el(`<div class="fae-row"><input class="fac-from" type="text" spellcheck="false" placeholder="Misspelled word" aria-label="Misspelled word"><input class="fac-to" type="text" spellcheck="false" placeholder="Correction" aria-label="Correction"><button class="cv-icon-btn fae-rm" type="button" title="Remove" aria-label="Remove">${icon('remove', 14)}</button></div>`);
    q<HTMLInputElement>(r, '.fac-from').value = from;
    q<HTMLInputElement>(r, '.fac-to').value = to;
    q(r, '.fae-rm').addEventListener('click', () => r.remove());
    rows.appendChild(r);
    return r;
  };
  for (const [a, b] of Object.entries(state.s.pairs).sort()) add(a, b);
  q(body, '.fac-add').addEventListener('click', () => { const r = add('', ''); q<HTMLInputElement>(r, '.fac-from').focus(); });
  d.body.appendChild(body);
  const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">OK</button>');
  cancel.addEventListener('click', () => d.close());
  ok.addEventListener('click', () => {
    const pairs: Record<string, string> = {};
    for (const r of Array.from(rows.children)) {
      const f = (r.querySelector('.fac-from') as HTMLInputElement).value.trim(), t = (r.querySelector('.fac-to') as HTMLInputElement).value.trim();
      if (f && t) pairs[f] = t;
    }
    state.s = { on: q<HTMLInputElement>(body, '.fac-on').checked, capitalize: q<HTMLInputElement>(body, '.fac-caps').checked, pairs };
    save(state.s);
    void push(state.s);
    d.close();
  });
  d.foot.append(cancel, ok);
  d.onClose = () => d.scrim.remove();
  d.open(ok);
}

export function installAutocorrect(ws: Workspace): void {
  const state = { s: load() };
  engine.on('hello', () => void push(state.s));
  ws.override('edit.autocorrect', () => open(state));
  ws.addCommands({ id: 'ui.autocorrectToggle', label: 'Enable Autocorrect', checked: () => state.s.on, run: () => { state.s = { ...state.s, on: !state.s.on }; save(state.s); void push(state.s); } });
  ws.addMenuItems({ menu: 'Edit', items: ['ui.autocorrectToggle'], at: 'end' });
}
