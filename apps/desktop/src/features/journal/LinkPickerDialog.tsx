import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { usePluginRevision, useSnapshot, useCampaign } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { commands, unwrap } from '@/shared/ipc/client';
import { Dialog } from '@/shared/ui/Dialog';
import { linkMarkdown, type LinkKind } from '@/shared/ui/Markdown';

/**
 * Picks something to link to inside a text or a table cell (ADR-0026): another entry, a map, a
 * character, a board, or an entry of an installed compendium. The result is Markdown, so it works
 * in a journal text and in a table cell alike.
 */
export function LinkPickerDialog({
  onClose,
  onPick,
  initialTab = 'elements',
}: {
  onClose: () => void;
  /** The Markdown link for the chosen target. */
  onPick: (markdown: string) => void;
  initialTab?: 'elements' | 'compendium';
}) {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const revision = usePluginRevision();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const [tab, setTab] = useState<'elements' | 'compendium'>(initialTab);
  const [compendiumKey, setCompendiumKey] = useState('');
  const compendia = useQuery({
    queryKey: ['compendia', campaign?.path, revision],
    queryFn: () => unwrap(commands.compendiumList()),
    enabled: tab === 'compendium' && campaign !== undefined && revision !== undefined,
  });
  const entries = useQuery({
    queryKey: ['compendium', campaign?.path, compendiumKey, revision],
    queryFn: () => {
      const [pluginId = '', id = ''] = compendiumKey.split('/');
      return unwrap(commands.compendiumEntries(pluginId, id, '', 0, 400));
    },
    enabled: tab === 'compendium' && compendiumKey !== '' && revision !== undefined,
  });

  const pick = (kind: LinkKind, id: string, text: string) => {
    onPick(linkMarkdown(kind, id, text));
    onClose();
  };

  return (
    <Dialog
      title={t('journal.insertLink')}
      closeLabel={t('common.cancel')}
      size="md"
      onClose={onClose}
      footer={
        <button type="button" onClick={onClose}>
          {t('common.cancel')}
        </button>
      }
    >
      <div className="link-picker">
        <div className="link-picker-tabs" role="tablist">
          {(['elements', 'compendium'] as const).map((value) => (
            <button
              key={value}
              type="button"
              role="tab"
              aria-selected={tab === value}
              onClick={() => {
                setTab(value);
              }}
            >
              {t(`journal.linkTargets.${value}`)}
            </button>
          ))}
        </div>
        {tab === 'elements' ? (
          <ul className="link-picker-list">
            {(snapshot?.journal ?? []).map((entry) => (
              <li key={entry.id}>
                <button
                  type="button"
                  onClick={() => {
                    pick('journal', entry.id, entry.title);
                  }}
                >
                  {t('journal.linkTargets.journal')}: {entry.title}
                </button>
              </li>
            ))}
            {(snapshot?.maps ?? []).map((map) => (
              <li key={map.id}>
                <button
                  type="button"
                  onClick={() => {
                    pick('map', map.id, map.name);
                  }}
                >
                  {t('journal.linkTargets.map')}: {map.name}
                </button>
              </li>
            ))}
            {(snapshot?.characters ?? []).map((character) => (
              <li key={character.id}>
                <button
                  type="button"
                  onClick={() => {
                    pick('character', character.id, character.name);
                  }}
                >
                  {t('journal.linkTargets.character')}: {character.name}
                </button>
              </li>
            ))}
            {(snapshot?.boards ?? []).map((board) => (
              <li key={board.id}>
                <button
                  type="button"
                  onClick={() => {
                    pick('board', board.id, board.name);
                  }}
                >
                  {t('journal.linkTargets.board')}: {board.name}
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <div className="link-picker-compendium">
            <select
              aria-label={t('journal.linkTargets.compendium')}
              value={compendiumKey}
              onChange={(e) => {
                setCompendiumKey(e.target.value);
              }}
            >
              <option value="">{t('compendiumPick.select')}</option>
              {(compendia.data ?? []).map((item) => (
                <option key={`${item.plugin_id}/${item.id}`} value={`${item.plugin_id}/${item.id}`}>
                  {labelText(item.name, lang)}
                </option>
              ))}
            </select>
            <ul className="link-picker-list">
              {(entries.data?.entries ?? []).map((entry) => (
                <li key={entry.id}>
                  <button
                    type="button"
                    onClick={() => {
                      const [pluginId = '', compendiumId = ''] = compendiumKey.split('/');
                      onPick(
                        `[${labelText(entry.name, lang).replace(/[[\]\\]/g, '')}](compendium:${pluginId}/${compendiumId}/${entry.id})`,
                      );
                      onClose();
                    }}
                  >
                    {labelText(entry.name, lang)}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Dialog>
  );
}
