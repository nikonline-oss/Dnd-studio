import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { CharacterDto, Intent } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { CodeEditor } from '@/shared/ui/CodeEditor';
import { ErrorText } from '@/shared/ui/ErrorText';
import { InlineEdit } from '@/shared/ui/InlineEdit';

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

/** Inspector for the selected character: name, owner, raw sheet and status transitions. */
export function CharacterEditor() {
  const { t } = useTranslation();
  const characterId = useSelection((s) => s.characterId);
  const character = useSnapshotSelect((s) => s.characters.find((c) => c.id === characterId));
  if (!character) return <p className="inspector-empty muted">{t('inspector.empty')}</p>;
  // Remount on a new version so the draft starts from the current sheet.
  return <Editor key={`${character.id}:${character.version}`} character={character} />;
}

function Editor({ character }: { character: CharacterDto }) {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const [sheet, setSheet] = useState(character.sheet_json);

  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const isOwner = character.owner !== null && character.owner === campaign?.user_id;
  const canEdit =
    isGm || (isOwner && (character.status === 'draft' || character.status === 'active'));
  const problem = sheetProblem(sheet);
  const users = useSnapshotSelect((s) => s.users) ?? [];

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
    <section className="character-editor" aria-label={t('characters.editor')}>
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

      <label className="sheet-label">{t('characters.sheet')}</label>
      <CodeEditor value={sheet} onChange={setSheet} label={t('characters.sheet')} language="json" />
      {problem && (
        <p role="alert" className="error-text">
          {t(`characters.sheetProblem.${problem}`)}
        </p>
      )}

      <div className="inline actions">
        {canEdit && (
          <button
            type="button"
            disabled={problem !== null || sheet === character.sheet_json || submit.isPending}
            onClick={() => {
              send({ kind: 'update_character', id: character.id, name: null, sheet_json: sheet });
            }}
          >
            {t('characters.saveSheet')}
          </button>
        )}
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
