import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import i18n from '@/shared/i18n';
import { ActivityBar } from './ActivityBar';

describe('ActivityBar names', () => {
  it.each(['en', 'ru'])('names skills and catalog in %s', async (language) => {
    await i18n.changeLanguage(language);
    render(
      <ActivityBar
        views={[{ id: 'skills', icon: 'book' }, { id: 'catalog', icon: 'book' }]}
        active={null}
        onSelect={() => undefined}
      />,
    );
    expect(screen.getByRole('button', { name: language === 'ru' ? 'Навыки' : 'Skills' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: language === 'ru' ? 'Каталог' : 'Catalog' })).toBeInTheDocument();
    await i18n.changeLanguage('en');
  });
});
