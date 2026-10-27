import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { CharacterDto, Intent } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { CodeEditor } from '@/shared/ui/CodeEditor';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { InlineEdit } from '@/shared/ui/InlineEdit';
import { SystemSheet } from './SystemSheet';

/** Checks what the core checks (ADR-0016) to give feedback before sending. */
export function sheetProblem(sheet: string): 'syntax' | 'not_object' | 'fraction' | null {
  let value: unknown;
  try {
    value = JSON.parse(sheet);
  } catch {
    return 'syntax';
  }
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return 'not_object';
  const integersOnly = (v: unknown): boolean => {
    if (typeof v === 'number') return Number.isSafeInteger(v);
    if (Array.isArray(v)) return v.every(integersOnly);
    if (typeof v === 'object' && v !== null) return Object.values(v).every(integersOnly);
    return true;
  };
  return integersOnly(value) ? null : 'fraction';
}

/**
 * A character: name, owner, sheet and status transitions — in the inspector (the selected one) or
 * in a center tab of its own (`characterId`), where the sheet has room. With a system plugin the
 * sheet is the plugin's layout (ADR-0023); the raw JSON stays available to GM roles.
 */
export function CharacterEditor({ characterId }: { characterId?: string } = {}) {
  const { t } = useTranslation();
  const selectedId = useSelection((s) => s.characterId);
  const id = characterId ?? selectedId;
  const character = useSnapshotSelect((s) => s.characters.find((c) => c.id === id));
  if (!character) return <p className="inspector-empty muted">{t('inspector.empty')}</p>;
  // Keyed by character only: a system sheet keeps its tab and focus across saved edits.
  return <Editor key={character.id} character={character} inTab={characterId !== undefined} />;
}

function Editor({ character, inTab }: { character: CharacterDto; inTab: boolean }) {
  const { t } = useTranslation();
  const openCharacter = useSelection((s) => s.openCharacter);
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();

  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const isOwner = character.owner !== null && character.owner === campaign?.user_id;
  const canEdit =
    isGm || (isOwner && (character.status === 'draft' || character.status === 'active'));
  const users = useSnapshotSelect((s) => s.users) ?? [];
  const rules = useSnapshotSelect(
    (s) => s.plugins.find((p) => p.id === s.system_plugin)?.system ?? null,
  );

  const send = (intent: Intent) => {
    submit.mutate({ intent, expectedVersion: character.version });
  };

  const transitions: { label: string; intent: Intent; allowed: boolean }[] = [
    {
      label: t('characters.submit'),
      intent: { kind: 'submit_character', id: character.id },
      allowed: character.status === 'draft' && (isGm || isOwner),
    },
    {
      label: t('characters.approve'),
      intent: { kind: 'approve_character', id: character.id },
      allowed: character.status === 'pending_approval' && isGm,
    },
    {
      label: t('characters.return'),
      intent: { kind: 'return_character', id: character.id },
      allowed: character.status === 'pending_approval' && isGm,
    },
    {
      label: t('characters.archive'),
      intent: { kind: 'archive_character', id: character.id },
      allowed: character.status !== 'archived' && isGm,
    },
  ];

  return (
    <section
      className={inTab ? 'character-editor character-editor-tab' : 'character-editor'}
      aria-label={t('characters.editor')}
    >
      {!inTab && (
        <button
          type="button"
          className="character-open-tab"
          onClick={() => {
            openCharacter(character.id);
          }}
        >
          <Icon name="fit" size={14} />
          {t('sheet.openTab')}
        </button>
      )}
      <dl className="properties">
        <dt>{t('inspector.name')}</dt>
        <dd>
          {canEdit ? (
            <InlineEdit
              value={character.name}
              label={t('maps.rename')}
              onCommit={(name) => {
                send({ kind: 'update_character', id: character.id, name, sheet_json: null });
              }}
            />
          ) : (
            character.name
          )}
        </dd>
        <dt>{t('characters.kind')}</dt>
        <dd>{t(`characters.kinds.${character.kind}`)}</dd>
        <dt>{t('characters.statusLabel')}</dt>
        <dd>{t(`characters.status.${character.status}`)}</dd>
        <dt>{t('characters.owner')}</dt>
        <dd>
          {isGm ? (
            <select
              aria-label={t('characters.owner')}
              value={character.owner ?? ''}
              onChange={(e) => {
                send({
                  kind: 'assign_character_owner',
                  id: character.id,
                  owner: e.target.value === '' ? null : e.target.value,
                });
              }}
            >
              <option value="">{t('characters.noOwner')}</option>
              {users.map((u) => (
                <option key={u.id} value={u.id}>
                  {u.name}
                </option>
              ))}
            </select>
          ) : (
            (users.find((u) => u.id === character.owner)?.name ?? t('characters.noOwner'))
          )}
        </dd>
      </dl>

      {rules ? (
        <>
          <SystemSheet character={character} rules={rules} canEdit={canEdit} />
          {isGm && (
            <details className="raw-sheet">
              <summary>{t('characters.rawSheet')}</summary>
              <RawSheet key={character.version} character={character} canEdit={canEdit} />
            </details>
          )}
        </>
      ) : (
        <RawSheet key={character.version} character={character} canEdit={canEdit} />
      )}

      <div className="inline actions">
        {transitions
          .filter((tr) => tr.allowed)
          .map((tr) => (
            <button
              key={tr.intent.kind}
              type="button"
              disabled={submit.isPending}
              onClick={() => {
                send(tr.intent);
              }}
            >
              {tr.label}
            </button>
          ))}
      </div>
      <ErrorText error={submit.error} />
    </section>
  );
}

/** The sheet as JSON (campaigns without a system, and GM roles under one). Keyed by version by the
 * caller, so the draft starts again from the saved sheet. */
function RawSheet({ character, canEdit }: { character: CharacterDto; canEdit: boolean }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const [sheet, setSheet] = useState(character.sheet_json);
  const problem = sheetProblem(sheet);
  return (
    <>
      <label className="sheet-label">{t('characters.sheet')}</label>
      <CodeEditor value={sheet} onChange={setSheet} label={t('characters.sheet')} language="json" />
      {problem && (
        <p role="alert" className="error-text">
          {t(`characters.sheetProblem.${problem}`)}
        </p>
      )}
      {canEdit && (
        <div className="inline actions">
          <button
            type="button"
            disabled={problem !== null || sheet === character.sheet_json || submit.isPending}
            onClick={() => {
              submit.mutate({
                intent: {
                  kind: 'update_character',
                  id: character.id,
                  name: null,
                  sheet_json: sheet,
                },
                expectedVersion: character.version,
              });
            }}
          >
            {t('characters.saveSheet')}
          </button>
        </div>
      )}
      <ErrorText error={submit.error} />
    </>
  );
}
