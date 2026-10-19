import { engine } from './engine/client';
import { FEATURES } from './features';
import { brandMark } from './ui/brand';
import { mountOnce, q, revealApp } from './ui/dom';
import { enhanceAllSelects } from './ui/dropdown';
import { icon } from './ui/icons';
import { Shell } from './ui/shell';
import { applyTheme } from './ui/themes';

const SHELL = `<div class="cv-app fd-app">
<header class="cv-titlebar">
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
    try { f.install(shell); } catch (err) { console.error('[formdes] a feature failed to install', err); }
  }
  shell.mountPanels();
  shell.viewport.readBackground();
  void engine.connect();
  revealApp();
}
