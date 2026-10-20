import '../styles/make.css';
import { Dialog } from '../ui/dialogs';
import { el, q } from '../ui/dom';
import type { Feature, Workspace } from '../ui/workspace';
import { BrandKits } from './make/brands';
import { Gallery } from './make/gallery';
import { MakeView } from './make/make-view';
import { SlotsView } from './make/slots';
import { links } from './make/store';

const LAYOUT = /\.(fdes|fdoc|idml)$/i;

function install(ws: Workspace): void {
  const make = new MakeView(ws);
  const slots = new SlotsView(ws);
  const makePage = el('<div class="mk-page"></div>');
  const slotsPage = el('<div class="mk-page mk-page-slots"></div>');
  makePage.append(make.root);
  slotsPage.append(slots.root);
  let gallery: Gallery | null = null;
  let kits: BrandKits | null = null;
  let dialog: Dialog | null = null;

  const homeShown = (): boolean => {
    const h = ws.root.querySelector<HTMLElement>('.fd-home');
    return !!h && !h.hidden;
  };

  const makeDialog = (): Dialog => {
    if (dialog) return dialog;
    const d = new Dialog('Make a Layout', 940);
    q(d.scrim, '.cv-dialog').classList.add('mk-dialog');
    d.onClose = () => { if (make.root.parentElement !== makePage) makePage.append(make.root); };
    dialog = d;
    return d;
  };

  const showMake = (): void => {
    if (homeShown()) {
      const d = makeDialog();
      if (make.root.parentElement !== d.body) d.body.append(make.root);
      d.open(q(make.root, '.mk-drop'));
      return;
    }
    if (dialog?.isOpen) return;
    if (make.root.parentElement !== makePage) makePage.append(make.root);
    ws.showPanel('make');
  };

  make.onMade = () => {
    if (dialog?.isOpen) dialog.close();
    ws.showPanel('make');
    requestAnimationFrame(() => requestAnimationFrame(() => make.scrollToReport()));
  };

  links.revealSlot = (slot) => {
    if (dialog?.isOpen) dialog.close();
    void slots.reveal(slot);
  };
  links.pickTemplate = () => (gallery ??= new Gallery()).open('pick');
  links.openBrands = (name) => void (kits ??= new BrandKits(ws)).open(name);

  ws.registerPanel({ id: 'make', label: 'Make', group: 'make', column: 'left', order: 10, create: () => makePage });
  ws.registerPanel({ id: 'slots', label: 'Slots', group: 'make', column: 'left', order: 20, create: () => slotsPage, onShow: () => slots.onShow() });

  const free = (combo: string): string[] => (ws.commands.forCombo(combo) ? [] : [combo]);
  ws.addCommands(
    { id: 'ui.make', label: 'Make a Layout…', keys: free('Mod+M'), run: showMake },
    { id: 'ui.templates', label: 'New from Template…', keys: free('Shift+Mod+N'), run: (params) => void (gallery ??= new Gallery()).open('create', typeof params?.category === 'string' ? params.category : undefined) },
    { id: 'ui.brands', label: 'Brand Kits…', run: () => links.openBrands() },
  );
  ws.addMenuItems({ menu: 'File', items: ['ui.make', 'ui.templates', 'ui.brands'], at: 'start' });

  ws.onDropFiles((files) => {
    if (make.consumeZoneDrop()) return true;
    const usable = files.filter((f) => !LAYOUT.test(f.name));
    if (!usable.length) return false;
    make.addFiles(usable);
    showMake();
    return true;
  });

  window.addEventListener('formdesign:make-pack', (e: Event) => {
    const detail = (e as CustomEvent<{ pack: string; title?: string }>).detail;
    if (!detail?.pack) return;
    showMake();
    void make.loadPack(detail.pack, detail.title);
  });
}

export const feature: Feature = { install };
