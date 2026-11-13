import { editor } from './editor/editor';
import { engine } from './engine/client';
import { FEATURES } from './features';
import { brandMark } from './ui/brand';
import { isMac, mountOnce, q, revealApp } from './ui/dom';
import { enhanceAllSelects } from './ui/dropdown';
import { icon } from './ui/icons';
import { Shell } from './ui/shell';
import { applyTheme } from './ui/themes';

const mod = isMac ? 'Cmd' : 'Ctrl';

const SHELL = `<div class="cv-app fd-app">
<header class="cv-titlebar" data-tauri-drag-region>
<button class="cv-brand" type="button" data-cmd="home" title="Home">${brandMark(18)}<span class="cv-brand-name">FormDesign</span></button>
<nav class="cv-menubar" aria-label="Main menu"></nav>
<div class="cv-zoom fd-title-tools">
<button class="cv-icon-btn" type="button" data-cmd="undo" title="Undo">${icon('undo', 16)}</button>
<button class="cv-icon-btn" type="button" data-cmd="redo" title="Redo">${icon('redo', 16)}</button>
<span class="cv-sep"></span>
<button class="cv-icon-btn" type="button" data-cmd="zoom-out" title="Zoom Out">${icon('zoomOut', 16)}</button>
<span class="cv-zoom-label fd-zoom-label"></span>
<button class="cv-icon-btn" type="button" data-cmd="zoom-in" title="Zoom In">${icon('zoomIn', 16)}</button>
<button class="cv-icon-btn" type="button" data-cmd="fit-spread" title="Fit Spread in Window">${icon('fitView', 16)}</button>
<span class="cv-sep"></span>
<button class="cv-btn fd-make-btn" type="button" data-cmd="make" title="Make a layout from files and a brief">${icon('makeLayout', 14)}<span>Make</span></button>
<button class="cv-icon-btn" type="button" data-cmd="claude" title="Claude Code (${mod}+Shift+J)" aria-label="Claude Code">${icon('agent', 16)}</button>
<button class="cv-icon-btn" type="button" data-cmd="terminal" title="Terminal (${mod}+J)" aria-label="Terminal">${icon('terminal', 16)}</button>
<button class="cv-icon-btn" type="button" data-cmd="settings" title="Preferences">${icon('settings', 16)}</button>
</div>
</header>
<div class="cv-main">
<div class="cv-options fd-control"></div>
<aside class="cv-rail" aria-label="Tools"><div class="cv-rail-tools"></div><div class="cv-colors fd-fillstroke"></div><div class="cv-rail-modes"></div><div class="fd-minirail" hidden></div></aside>
<section class="fd-left-col" aria-label="Left panels" hidden></section>
<div class="cv-stage-col">
<div class="cv-doc-tabs"><div class="cv-tabs"><div class="cv-tab-strip" role="tablist"></div><button class="cv-icon-btn" type="button" data-cmd="new" title="New Document">${icon('add', 16)}</button></div></div>
<main class="cv-stage fd-stage">
<canvas class="cv-canvas fd-tiles"></canvas>
<canvas class="cv-canvas fd-hilite"></canvas>
<canvas class="cv-canvas cv-overlay-canvas fd-overlay"></canvas>
<canvas class="cv-ruler cv-ruler-h" hidden></canvas>
<canvas class="cv-ruler cv-ruler-v" hidden></canvas>
<div class="cv-drop" hidden></div>
</main>
<section class="fd-dock" aria-label="Terminal" hidden>
<div class="fd-dock-rsz" role="separator" aria-orientation="horizontal" aria-label="Resize the terminal"></div>
<div class="fd-dock-head">
<div class="fd-dock-tabs" role="tablist" aria-label="Terminals"></div>
<span class="fd-dock-spacer"></span>
<button type="button" class="cv-icon-btn fd-dock-new" title="New Terminal" aria-label="New Terminal">${icon('plus', 15)}</button>
<button type="button" class="cv-icon-btn fd-dock-chat" title="Chat with Claude about your document" aria-label="Chat">${icon('agentChat', 15)}</button>
<button type="button" class="cv-icon-btn fd-dock-claude" title="Claude Code" aria-label="Claude Code">${icon('agent', 15)}</button>
<button type="button" class="cv-icon-btn fd-dock-restart" title="Restart this tab" aria-label="Restart this tab" disabled>${icon('refresh', 15)}</button>
<button type="button" class="cv-icon-btn fd-dock-hide" title="Hide Terminal (${mod}+J); programs keep running" aria-label="Hide Terminal">${icon('close', 15)}</button>
</div>
<div class="fd-dock-stage"><p class="fd-dock-empty cv-note">No terminal is open. Start a shell with the plus button, Claude Code with the robot button, or Chat with the speech bubble.</p></div>
<form class="fd-dock-ask" autocomplete="off">
<span class="fd-dock-ask-mark">${icon('agent', 15)}</span>
<input class="cv-input fd-dock-ask-input" type="text" spellcheck="false" placeholder="Ask Claude Code to work on your document" aria-label="Ask Claude Code">
<button type="submit" class="cv-icon-btn fd-dock-ask-send" title="Send to Claude Code" aria-label="Send to Claude Code">${icon('send', 14)}</button>
</form>
</section>
</div>
<section class="cv-props-col" aria-label="Properties"></section>
<div class="cv-resizer" role="separator" aria-orientation="vertical" aria-label="Resize the panels"></div>
<section class="cv-layers" aria-label="Panels"></section>
</div>
<footer class="cv-status fd-status"><span class="cv-st-engine fd-st-engine"></span><span class="fd-st-page"></span><span class="cv-st-hint fd-st-hint"></span><span class="fd-st-version"></span></footer>
</div>`;

export function startApp(): void {
  applyTheme();
  const host = document.getElementById('app');
  if (!host) return;
  mountOnce(host, SHELL);
  enhanceAllSelects();
  const root = q(host, '.fd-app');
  const shell = new Shell(root);
  for (const f of FEATURES) {
    try { f.install(shell); } catch (err) { console.error('[formdesign] a feature failed to install', err); }
  }
  shell.mountPanels();
  shell.viewport.readBackground();
  void engine.connect();
  const first = new Promise((done) => { const off = editor.on('ready', () => { off(); done(null); }); });
  void Promise.race([Promise.all([document.fonts.ready, first]), new Promise((done) => setTimeout(done, 2500))]).then(revealApp, revealApp);
}
