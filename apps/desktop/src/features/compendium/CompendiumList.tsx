import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useCampaign } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { commands, unwrap } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { NavSection } from '@/shared/ui/NavSection';

/** Navigator: the compendia of the campaign's plugins (CLAUDE.md §12, ADR-0025). */
export function CompendiumList() {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const { data: campaign } = useCampaign();
  const active = useSelection((s) => s.active);
  const openCompendium = useSelection((s) => s.openCompendium);
  const compendia = useQuery({
    // Plugins change rarely; the list is read again when the campaign does.
    queryKey: ['compendia', campaign?.path],
    queryFn: () => unwrap(commands.compendiumList()),
  });
  const list = compendia.data ?? [];

  return (
    <NavSection icon="book" title={t('compendia.title')} count={list.length}>
      {compendia.isSuccess && list.length === 0 && (
        <p className="nav-empty">{t('compendia.empty')}</p>
      )}
      <ul className="nav-list" aria-label={t('compendia.title')}>
        {list.map((c) => {
          const key = `${c.plugin_id}/${c.id}`;
          return (
            <li
              key={key}
              className="nav-row"
              aria-current={active?.kind === 'compendium' && active.id === key}
              onClick={() => {
                openCompendium(key);
              }}
            >
              <Icon name="book" className="row-icon" />
              <span className="nav-row-name">{labelText(c.name, lang)}</span>
              <span className="muted">{c.count}</span>
            </li>
          );
        })}
      </ul>
      <ErrorText error={compendia.error} />
    </NavSection>
  );
}
