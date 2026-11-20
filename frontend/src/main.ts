import { migrateStorage } from './ui/storage-migrate';
import { startApp } from './app';
import { startLanguage, startPageTranslator } from './ui/i18n';
import { loadWindowKey } from './engine/window-key';

migrateStorage();
void loadWindowKey().then(() => startLanguage()).finally(() => {
  startApp();
  startPageTranslator();
});
