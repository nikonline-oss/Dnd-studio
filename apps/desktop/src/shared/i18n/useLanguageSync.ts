import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { detectLanguage } from './index';
import { useSettings } from '@/shared/settings/settings';

/** Applies the language preference (`system` follows the OS/webview locale). */
export function useLanguageSync() {
  const language = useSettings((s) => s.language);
  const { i18n } = useTranslation();
  useEffect(() => {
    const target = language === 'system' ? detectLanguage(navigator.language) : language;
    if (i18n.language !== target) void i18n.changeLanguage(target);
    document.documentElement.lang = target;
  }, [language, i18n]);
}
