import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { entryMarkdown } from './entryMarkdown';
import { CompendiumCharacter } from './CompendiumCharacter';
import { useCampaign, usePluginRevision, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { commands, unwrap } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Markdown } from '@/shared/ui/Markdown';

/** Entries shown at once; "more" pages in the next ones. */
const PAGE = 200;

/**
 * A compendium in a center tab: search on the left, the chosen entry on the right. GM roles copy
 * an entry into the journal (a GM-only entry, to edit and share like any other).
 */
export function CompendiumView({ compendiumKey }: { compendiumKey: string }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const [pluginId = '', compendiumId = ''] = compendiumKey.split('/');
  const [query, setQuery] = useState('');
  const [limit, setLimit] = useState(PAGE);
  const [chosenId, setChosenId] = useState<string | null>(null);
  const { data: campaign } = useCampaign();
  const revision = usePluginRevision();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const submit = useSubmitIntent();
  const openJournal = useSelection((s) => s.openJournal);
  const page = useQuery({
    queryKey: ['compendium', campaign?.path, pluginId, compendiumId, revision, query, limit],
    queryFn: () => unwrap(commands.compendiumEntries(pluginId, compendiumId, query, 0, limit)),
    placeholderData: (previous, previousQuery) => {
      if (!previousQuery) return undefined;
      const key = previousQuery.queryKey;
      return key[1] === campaign?.path &&
        key[2] === pluginId &&
        key[3] === compendiumId &&
        key[4] === revision
        ? previous
        : undefined;
    },
    enabled: campaign !== undefined && campaign !== null && revision !== undefined,
  });
  const entries = page.data?.entries ?? [];
  const chosen = page.isError ? null : (entries.find((entry) => entry.id === chosenId) ?? null);
  const total = page.data?.total ?? 0;

  return (
    <section className="compendium-view" aria-label={t('compendia.title')}>
      <div className="compendium-list">
        <input
          type="search"
          aria-label={t('picker.search')}
          placeholder={t('picker.search')}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setLimit(PAGE);
          }}
        />
        <ul role="listbox" aria-label={t('compendia.entries')}>
          {entries.map((entry) => (
            <li
              key={entry.id}
              role="option"
              aria-selected={chosen?.id === entry.id}
              onClick={() => {
                setChosenId(entry.id);
              }}
            >
              {labelText(entry.name, lang)}
            </li>
          ))}
        </ul>
        {page.isSuccess && entries.length === 0 && <p className="muted">{t('picker.nothing')}</p>}
        {entries.length < total && (
          <button
            type="button"
            onClick={() => {
              setLimit((l) => l + PAGE);
            }}
          >
            {t('compendia.more', { count: total - entries.length })}
          </button>
        )}
      </div>
      <article className="compendium-entry">
        {chosen ? (
          <>
            <Markdown source={entryMarkdown(chosen, lang)} />
            {isGm && (
              <CompendiumCharacter
                key={`${campaign.path}/${compendiumKey}/${revision}/${chosen.id}`}
                entry={chosen}
              />
            )}
            {isGm && (
              <button
                type="button"
                disabled={submit.isPending}
                onClick={() => {
                  const id = uuidv7();
                  submit.mutate(
                    {
                      intent: {
                        kind: 'create_journal_entry',
                        id,
                        folder_id: null,
                        title: labelText(chosen.name, lang),
                        markdown: entryMarkdown(chosen, lang),
                        visibility: 'gm',
                        players_can_edit: false,
                      },
                    },
                    {
                      onSuccess: () => {
                        openJournal(id);
                      },
                    },
                  );
                }}
              >
                {t('compendia.toJournal')}
              </button>
            )}
          </>
        ) : (
          <p className="muted">{t('compendia.pick')}</p>
        )}
        <ErrorText error={page.error ?? submit.error} />
      </article>
    </section>
  );
}
