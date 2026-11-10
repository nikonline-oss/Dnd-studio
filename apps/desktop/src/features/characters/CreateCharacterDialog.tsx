import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSubmitIntent } from '@/shared/game/useGame';
import type { CharacterKind } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { Dialog } from '@/shared/ui/Dialog';
import { ErrorText } from '@/shared/ui/ErrorText';

/**
 * Creating a character is a small form, not an instant "New PC" that has to be renamed afterwards:
 * the name (and, for GM roles, the kind) is chosen before the intent is sent. Players only create
 * their own PC, so the kind is fixed for them.
 */
export function CreateCharacterDialog({
  kind: initialKind,
  canChooseKind,
  onClose,
  onCreated,
}: {
  kind: CharacterKind;
  canChooseKind: boolean;
  onClose: () => void;
  onCreated: (id: string) => void;
}) {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const [kind, setKind] = useState<CharacterKind>(initialKind);
  const [name, setName] = useState(() =>
    initialKind === 'npc' ? t('characters.newNpc') : t('characters.newPc'),
  );
  // Switching the kind in the dialog renames the default, but never a name the user typed.
  const [typed, setTyped] = useState(false);
  const changeKind = (next: CharacterKind) => {
    setKind(next);
    if (!typed) setName(next === 'npc' ? t('characters.newNpc') : t('characters.newPc'));
  };

  const create = () => {
    if (!campaign) return;
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_character',
          id,
          character_kind: kind,
          name: name.trim(),
          // Players may only create characters they own; NPCs belong to nobody.
          owner: kind === 'npc' ? null : campaign.user_id,
          sheet_json: '{}',
        },
      },
      {
        onSuccess: () => {
          onCreated(id);
          onClose();
        },
      },
    );
  };

  return (
    <Dialog
      title={kind === 'npc' ? t('characters.addNpc') : t('characters.addPc')}
      closeLabel={t('common.cancel')}
      size="sm"
      dismissible={!submit.isPending}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn-primary"
            disabled={submit.isPending || !name.trim()}
            onClick={create}
          >
            {t('common.create')}
          </button>
        </>
      }
    >
      <label>
        {t('characters.name')}
        <input
          value={name}
          maxLength={200}
          onChange={(event) => {
            setTyped(true);
            setName(event.target.value);
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && name.trim() && !submit.isPending) {
              event.preventDefault();
              create();
            }
          }}
        />
      </label>
      {canChooseKind && (
        <fieldset className="dialog-choice">
          <legend>{t('characters.kind')}</legend>
          {(['pc', 'npc'] as const).map((option) => (
            <label key={option}>
              <input
                type="radio"
                name="character-kind"
                checked={kind === option}
                onChange={() => {
                  changeKind(option);
                }}
              />
              {t(`characters.kinds.${option}`)}
            </label>
          ))}
        </fieldset>
      )}
      <p className="muted">{kind === 'npc' ? t('characters.npcHint') : t('characters.pcHint')}</p>
      <ErrorText error={submit.error} />
    </Dialog>
  );
}
