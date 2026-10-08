import type { Feature } from '../ui/workspace';
import { feature as document } from './document';
import { feature as make } from './make';
import { feature as pages } from './pages';
import { feature as properties } from './properties';
import { feature as reader } from './reader';
import { rulersFeature as rulers } from './rulers';
import { installGlyphs } from './glyphs';
import { installTabs } from './tabs';
import { installTable } from './table';
import { installBook } from './book';
import { installStoryEditor } from './story';
import { installReview } from './review';
import { installSeparations } from './separations';

export const FEATURES: Feature[] = [make, properties, pages, document, reader, rulers, { install: installGlyphs }, { install: installTabs }, { install: installTable }, { install: installBook },
  { install: installStoryEditor }, { install: installReview }, { install: installSeparations }];
