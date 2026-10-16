import type { Feature } from '../ui/workspace';
import { feature as document } from './document';
import { feature as make } from './make';
import { feature as pages } from './pages';
import { feature as properties } from './properties';
import { rulersFeature as rulers } from './rulers';
import { installGlyphs } from './glyphs';
import { installTabs } from './tabs';
import { installTable } from './table';
import { installBook } from './book';
import { installStoryEditor } from './story';
import { installReview } from './review';
import { installSeparations } from './separations';
import { installFind } from './find';
import { installSpelling } from './spelling';
import { installPathType } from './pathtype';
import { installInteractive } from './interactive';
import { installAnimation } from './animation';
import { installAssignments } from './assignments';
import { installXml } from './xml';
import { installLibrary } from './library';
import { installDictionaries } from './dictionaries';
import { installProof } from './proof';
import { installShortcuts } from './shortcuts';
import { installWorkspaces } from './workspaces';
import { installClipboard } from './clipboard';
import { installTextOptions } from './text-options';
import { installTerminal } from './terminal';
import { installConnect } from './connect';
import { installColourTools } from './colour-tools';
import { installAutocorrect } from './autocorrect';
import { installPdfComments } from './pdf-comments';
import { installReviewShare } from './review-share';
import { installWordImport } from './word-import';
import { installRenderer } from './renderer';
import { installTableStyles } from './table-styles';
import { installAssistant } from './assistant';
import { installAiTools } from './ai-tools';
import { installCloudFonts } from './cloud-fonts';
import { installPlugins } from './plugins';
import { installClaude } from './claude';
import { installActions } from './actions';

export const FEATURES: Feature[] = [make, properties, pages, document, rulers, { install: installGlyphs }, { install: installTabs }, { install: installTable }, { install: installBook },
  { install: installStoryEditor }, { install: installReview }, { install: installSeparations }, { install: installFind }, { install: installSpelling }, { install: installPathType }, { install: installInteractive }, { install: installAnimation }, { install: installAssignments }, { install: installXml }, { install: installLibrary },
  { install: installDictionaries }, { install: installProof }, { install: installWorkspaces }, { install: installShortcuts }, { install: installClipboard }, { install: installTextOptions }, { install: installTerminal }, { install: installConnect }, { install: installColourTools }, { install: installAutocorrect },
  { install: installPdfComments }, { install: installReviewShare }, { install: installWordImport }, { install: installRenderer }, { install: installTableStyles }, { install: installAssistant }, { install: installAiTools },
  { install: installCloudFonts }, { install: installPlugins }, { install: installClaude }, { install: installActions }];
