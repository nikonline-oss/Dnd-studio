import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from './locales/en.json';
import ru from './locales/ru.json';

export const LANGUAGES = ['ru', 'en'] as const;
export type Language = (typeof LANGUAGES)[number];

export const resources = { en: { translation: en }, ru: { translation: ru } } as const;

/** `ru-RU` → `ru`; anything unsupported → `en`. */
export function detectLanguage(locale: string | undefined): Language {
  return locale?.toLowerCase().startsWith('ru') ? 'ru' : 'en';
}

void i18n.use(initReactI18next).init({
  resources,
  lng: detectLanguage(typeof navigator === 'undefined' ? undefined : navigator.language),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;
