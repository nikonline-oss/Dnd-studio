import { useEffect } from 'react';
import { useSettings } from '@/shared/settings/settings';

/**
 * Applies the theme preference to `<html data-theme>`. In `system` mode the attribute is removed
 * and `prefers-color-scheme` decides (see tokens.css).
 */
export function useThemeSync() {
  const theme = useSettings((s) => s.theme);
  useEffect(() => {
    const root = document.documentElement;
    if (theme === 'system') {
      delete root.dataset.theme;
    } else {
      root.dataset.theme = theme;
    }
  }, [theme]);
}
