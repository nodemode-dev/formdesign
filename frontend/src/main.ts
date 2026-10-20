import { migrateStorage } from './ui/storage-migrate';
import { startApp } from './app';
import { startLanguage, startPageTranslator } from './ui/i18n';

migrateStorage();
void startLanguage().finally(() => {
  startApp();
  startPageTranslator();
});
