import { startApp } from './app';
import { startLanguage, startPageTranslator } from './ui/i18n';

void startLanguage().finally(() => {
  startApp();
  startPageTranslator();
});
