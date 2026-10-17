import { editor } from '../editor/editor';
import { el, q, setText } from '../ui/dom';
import { t } from '../ui/i18n';
import type { Workspace } from '../ui/workspace';

export function installDrawMode(ws: Workspace): void {
  const chip = el<HTMLButtonElement>('<button type="button" class="fd-st-mode"></button>');
  const status = q(ws.root, '.fd-status');
  status.insertBefore(chip, q(status, '.fd-st-hint').nextSibling);

  const sync = (): void => {
    const parts: string[] = [];
    if (editor.isolated !== null) parts.push(t('Isolation Mode'));
    if (editor.drawMode === 'behind') parts.push(t('Draw Behind'));
    else if (editor.drawMode === 'inside') parts.push(t('Draw Inside'));
    setText(chip, parts.join(' · '));
    chip.title = editor.isolated !== null ? t('Leave isolation mode (Esc)') : t('Draw normally (Shift+D cycles the drawing modes)');
  };
  chip.addEventListener('click', () => {
    if (editor.isolated !== null) void editor.exec('object.isolate', { exit: true });
    else if (editor.drawMode !== 'normal') void editor.exec('object.drawMode', { mode: 'normal' });
  });
  editor.on('mode', sync);
  sync();
}
