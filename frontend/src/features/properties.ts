import '../styles/properties.css';
import type { Feature, Workspace } from '../ui/workspace';
import { ControlBar } from './properties/control';
import { ctx } from './properties/ctx';
import { PropertiesPanel } from './properties/props';
import { StylesPanel } from './properties/styles';
import { SwatchesPanel } from './properties/swatches';
import { CharacterPanel, ParagraphPanel } from './properties/textpanels';

export const feature: Feature = {
  install(ws: Workspace) {
    ws.registerPanel({ id: 'properties', label: 'Properties', group: 'props', column: 'side', order: 10, create: () => new PropertiesPanel(ws).root, onShow: () => ctx.schedule() });
    ws.registerPanel({ id: 'character', label: 'Character', group: 'props', column: 'side', order: 20, create: () => new CharacterPanel().root, onShow: () => ctx.schedule() });
    ws.registerPanel({ id: 'paragraph', label: 'Paragraph', group: 'props', column: 'side', order: 30, create: () => new ParagraphPanel().root, onShow: () => ctx.schedule() });
    ws.registerPanel({ id: 'swatches', label: 'Swatches', group: 'swatches', column: 'dock', order: 10, create: () => new SwatchesPanel().root, onShow: () => ctx.schedule() });
    ws.registerPanel({ id: 'paraStyles', label: 'Paragraph Styles', group: 'swatches', column: 'dock', order: 20, create: () => new StylesPanel('paragraph').root, onShow: () => ctx.schedule() });
    ws.registerPanel({ id: 'charStyles', label: 'Character Styles', group: 'swatches', column: 'dock', order: 30, create: () => new StylesPanel('character').root, onShow: () => ctx.schedule() });
    ws.setControl('properties', new ControlBar().root, 10);
    ws.addCommands(
      { id: 'ui.type.character', label: 'Character', keys: ['Mod+T'], run: () => ws.showPanel('character') },
      { id: 'ui.type.paragraph', label: 'Paragraph', keys: ['Alt+Mod+T'], run: () => ws.showPanel('paragraph') },
      { id: 'ui.type.paraStyles', label: 'Paragraph Styles', keys: ['F11'], run: () => ws.showPanel('paraStyles') },
      { id: 'ui.type.charStyles', label: 'Character Styles', keys: ['Shift+F11'], run: () => ws.showPanel('charStyles') },
    );
    ws.addMenuItems({ menu: 'Type', at: 'start', items: ['ui.type.character', 'ui.type.paragraph', '-', 'ui.type.paraStyles', 'ui.type.charStyles'] });
    ctx.schedule();
  },
};
