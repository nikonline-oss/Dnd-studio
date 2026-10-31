import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import type {
  TemplateParam,
  TemplateParamKind,
  TriggerDef,
  TriggerTemplateDto,
} from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { Dialog } from '@/shared/ui/Dialog';
import { ElementField, type ElementKind } from '@/shared/ui/ElementPicker';
import { ErrorText } from '@/shared/ui/ErrorText';

const ELEMENT_KINDS: Partial<Record<TemplateParamKind, ElementKind>> = {
  zone: 'zone',
  board_node: 'board_node',
  board: 'board',
  map: 'map',
  journal: 'journal',
  token: 'token',
};

/** A template's definition with its `{{param}}` blanks filled in (in every string). */
export function fillTemplate(definition: unknown, values: Record<string, string>): unknown {
  if (typeof definition === 'string') {
    return Object.entries(values).reduce(
      (text, [id, value]) => text.split(`{{${id}}}`).join(value),
      definition,
    );
  }
  if (Array.isArray(definition)) return definition.map((item) => fillTemplate(item, values));
  if (typeof definition === 'object' && definition !== null) {
    return Object.fromEntries(
      Object.entries(definition).map(([key, value]) => [key, fillTemplate(value, values)]),
    );
  }
  return definition;
}

/**
 * Trigger templates of the installed plugins (ADR-0024): pick one, fill in its blanks, and a
 * trigger is created from it. The core checks it like any other.
 */
export function TemplateDialog({
  onClose,
  boardId,
  position = { x: 0, y: 0 },
}: {
  onClose: () => void;
  boardId: string;
  position?: { x: number; y: number };
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const submit = useSubmitIntent();
  const { data: campaign } = useCampaign();
  const boardExists = useSnapshotSelect((s) => s.boards.some((b) => b.id === boardId));
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const openTrigger = useSelection((s) => s.openTrigger);
  const templates =
    useSnapshotSelect((s) =>
      s.plugins.flatMap((p) => (p.trigger_templates ?? []).map((tpl) => ({ plugin: p.id, tpl }))),
    ) ?? [];
  const [chosen, setChosen] = useState<TriggerTemplateDto | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const complete = chosen?.params.every((p) => (values[p.id] ?? '').trim() !== '') ?? false;

  const create = async () => {
    if (!chosen || !complete || submit.isPending) return;
    const id = uuidv7();
    // The blanks make it a TriggerDef; the core checks every field.
    const definition = fillTemplate(chosen.definition, values) as TriggerDef;
    try {
      await submit.mutateAsync({
        intent: {
          kind: 'create_trigger',
          id,
          definition: { ...definition, map_ids: definition.map_ids ?? [] },
        },
      });
      await submit.mutateAsync({
        intent: {
          kind: 'create_board_node',
          id: uuidv7(),
          board_id: boardId,
          content: { type: 'trigger', id },
          x: position.x,
          y: position.y,
          width: 4160,
          height: 2400,
          visibility: 'gm',
        },
      });
      openTrigger(id);
      onClose();
    } catch {
      // Keep the filled template visible; the mutation displays the rejection.
    }
  };

  if (!isGm || !boardExists) return null;
  return (
    <Dialog title={t('triggers.fromTemplate')} closeLabel={t('picker.cancel')} onClose={onClose}>
      {templates.length === 0 && <p className="muted">{t('triggers.noTemplates')}</p>}
      <ul className="picker-list template-list" role="listbox" aria-label={t('triggers.templates')}>
        {templates.map(({ plugin, tpl }) => (
          <li
            key={`${plugin}:${tpl.id}`}
            role="option"
            aria-selected={chosen === tpl}
            onClick={() => {
              setChosen(tpl);
              setValues({});
            }}
          >
            <span className="picker-name">{labelText(tpl.name, lang)}</span>
            <span className="picker-detail muted">
              {tpl.description ? labelText(tpl.description, lang) : plugin}
            </span>
          </li>
        ))}
      </ul>
      {chosen && (
        <div className="template-params">
          {chosen.params.map((param) => (
            <ParamField
              key={param.id}
              param={param}
              value={values[param.id] ?? ''}
              onChange={(value) => {
                setValues((v) => ({ ...v, [param.id]: value }));
              }}
            />
          ))}
        </div>
      )}
      {!complete && (
        <p role="status">
          {t('triggers.templateIncomplete', {
            defaultValue: 'Choose a template and fill every parameter to create its board card.',
          })}
        </p>
      )}
      <footer className="dialog-actions">
        <button type="button" onClick={onClose}>
          {t('picker.cancel')}
        </button>
        <button
          type="button"
          className="btn-primary"
          disabled={!chosen || !complete || submit.isPending}
          onClick={() => {
            void create();
          }}
        >
          {t('triggers.createFromTemplate')}
        </button>
      </footer>
      <ErrorText error={submit.error} />
    </Dialog>
  );
}

function ParamField({
  param,
  value,
  onChange,
}: {
  param: TemplateParam;
  value: string;
  onChange: (value: string) => void;
}) {
  const { i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const label = labelText(param.label, lang);
  const rules = useSnapshotSelect(
    (s) => s.plugins.find((p) => p.id === s.system_plugin)?.system ?? null,
  );
  const element = ELEMENT_KINDS[param.kind];
  if (element) {
    return (
      <ElementField
        label={label}
        kinds={[element]}
        value={value ? { kind: element, id: value } : null}
        onChange={({ id }) => {
          onChange(id);
        }}
      />
    );
  }
  const options =
    param.kind === 'roll'
      ? rules?.rolls.map((r) => ({ id: r.id, label: r.label }))
      : param.kind === 'resource'
        ? rules?.resources.map((r) => ({ id: r.id, label: r.label }))
        : param.kind === 'condition'
          ? rules?.conditions?.map((c) => ({ id: c.id, label: c.label }))
          : undefined;
  return (
    <label className="trigger-field">
      <span>{label}</span>
      {options ? (
        <select
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
          }}
        >
          <option value="" />
          {options.map((o) => (
            <option key={o.id} value={o.id}>
              {labelText(o.label, lang)}
            </option>
          ))}
        </select>
      ) : (
        <input
          type={param.kind === 'number' ? 'number' : 'text'}
          value={value}
          onChange={(e) => {
            onChange(e.target.value);
          }}
        />
      )}
    </label>
  );
}
