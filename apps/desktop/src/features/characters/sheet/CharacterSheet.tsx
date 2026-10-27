import type { TFunction } from 'i18next';
import {
  createContext,
  memo,
  useCallback,
  useContext,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from 'react';
import { useTranslation } from 'react-i18next';
import { Icon } from '@/shared/ui/Icon';
import {
  clampText,
  defaultValue,
  formatComputed,
  getPath,
  parseInteger,
  resolveDice,
  resolveLabel,
  schemaAt,
  setPath,
  splitPath,
  widgetFor,
  type FieldNode,
  type JsonSchema,
  type Label,
  type ListNode,
  type SheetData,
  type SheetNode,
  type SheetResource,
  type SheetRoll,
  type TabsNode,
} from './model';

export interface CharacterSheetProps {
  /** The system plugin's layout: one root node or several. */
  layout: SheetNode | SheetNode[];
  schema?: JsonSchema | undefined;
  data: SheetData;
  /** Formula fields, already evaluated by the core. */
  computed: Record<string, number>;
  rolls: SheetRoll[];
  resources: SheetResource[];
  /** `false` shows values without inputs; roll buttons still work. */
  canEdit: boolean;
  /** The whole next sheet; untouched branches keep their identity. */
  onChange: (nextData: SheetData) => void;
  onRoll: (rollId: string) => void;
}

interface Actions {
  lang: string;
  t: TFunction;
  canEdit: boolean;
  schema: JsonSchema | undefined;
  emit: (path: string[], value: unknown) => void;
  roll: (rollId: string) => void;
}

interface Values {
  data: SheetData;
  computed: Record<string, number>;
  rolls: SheetRoll[];
  resources: SheetResource[];
}

// Two contexts: fields subscribe only to the stable one, so new computed values (which arrive after
// every edit) re-render the few nodes that show them, not every input.
const ActionsContext = createContext<Actions | null>(null);
const ValuesContext = createContext<Values>({ data: {}, computed: {}, rolls: [], resources: [] });

function useActions(): Actions {
  const actions = useContext(ActionsContext);
  if (actions === null) throw new Error('sheet nodes must be rendered inside <CharacterSheet>');
  return actions;
}

/** A declarative character sheet (CLAUDE.md §8): renders a plugin layout over the sheet data. */
export function CharacterSheet({
  layout,
  schema,
  data,
  computed,
  rolls,
  resources,
  canEdit,
  onChange,
  onRoll,
}: CharacterSheetProps) {
  const { t, i18n } = useTranslation();
  const lang = i18n.resolvedLanguage ?? i18n.language;

  // Edits are applied to the latest data, including edits the parent has not echoed back yet, so
  // two quick commits (blur one field, click the next) compose instead of overwriting each other.
  const latest = useRef({ data, onChange, onRoll });
  useLayoutEffect(() => {
    latest.current = { data, onChange, onRoll };
  }, [data, onChange, onRoll]);

  const emit = useCallback((path: string[], value: unknown) => {
    const next = setPath(latest.current.data, path, value);
    latest.current.data = next;
    latest.current.onChange(next);
  }, []);
  const roll = useCallback((rollId: string) => {
    latest.current.onRoll(rollId);
  }, []);

  const roots = Array.isArray(layout) ? layout : [layout];
  return (
    <ActionsContext value={{ lang, t, canEdit, schema, emit, roll }}>
      <ValuesContext value={{ data, computed, rolls, resources }}>
        <div className={canEdit ? 'sheet' : 'sheet sheet-readonly'}>
          <Nodes nodes={roots} scope={data} base={[]} />
        </div>
      </ValuesContext>
    </ActionsContext>
  );
}

interface ScopeProps {
  /** The object relative paths resolve against: the sheet, or a list item. */
  scope: unknown;
  /** Absolute path of `scope` in the sheet. */
  base: string[];
}

function Nodes({ nodes, scope, base }: ScopeProps & { nodes: unknown }) {
  if (!Array.isArray(nodes)) return null;
  return (nodes as SheetNode[]).map((node, i) => (
    <NodeView key={i} node={node} scope={scope} base={base} />
  ));
}

function NodeView({ node, scope, base }: ScopeProps & { node: SheetNode }) {
  const { lang } = useActions();
  switch (node.type) {
    case 'tabs':
      return <Tabs node={node} scope={scope} base={base} />;
    case 'section': {
      const label = resolveLabel(node.label, lang);
      return (
        <section className="sheet-section" aria-label={label || undefined}>
          {label && <h3 className="sheet-section-title">{label}</h3>}
          <Nodes nodes={node.children} scope={scope} base={base} />
        </section>
      );
    }
    case 'row':
      return (
        <div className="sheet-row">
          <Nodes nodes={node.children} scope={scope} base={base} />
        </div>
      );
    case 'grid': {
      const columns = Number.isInteger(node.columns) ? Math.min(Math.max(node.columns, 1), 12) : 1;
      return (
        // At most `columns`; fewer on a narrow panel (styles.css).
        <div className="sheet-grid" style={{ '--sheet-cols': columns } as CSSProperties}>
          <Nodes nodes={node.children} scope={scope} base={base} />
        </div>
      );
    }
    case 'field': {
      const segments = splitPath(node.path);
      return (
        <Field
          node={node}
          value={getPath(scope, segments)}
          path={[...base, ...segments].join('.')}
        />
      );
    }
    case 'computed':
      return <Computed name={node.name} label={node.label} format={node.format} />;
    case 'roll':
      return <RollButton rollId={node.roll} label={node.label} />;
    case 'resource':
      return <Resource id={node.resource} />;
    case 'list': {
      const segments = splitPath(node.path);
      return <List node={node} value={getPath(scope, segments)} path={[...base, ...segments]} />;
    }
    case 'text':
      return <p className="sheet-text">{resolveLabel(node.text, lang)}</p>;
    default:
      return <Unsupported type={(node as { type?: unknown }).type} />;
  }
}

function Unsupported({ type }: { type: unknown }) {
  const { t } = useActions();
  return (
    <div className="sheet-unsupported" role="note">
      {t('sheet.unsupported', { type: typeof type === 'string' ? type : '?' })}
    </div>
  );
}

function Tabs({ node, scope, base }: ScopeProps & { node: TabsNode }) {
  const { lang } = useActions();
  const id = useId();
  const [selected, setSelected] = useState(0);
  const tabs = Array.isArray(node.tabs) ? node.tabs : [];
  const listRef = useRef<HTMLDivElement>(null);
  const active = Math.min(selected, tabs.length - 1);
  const current = tabs[active];
  if (current === undefined) return null;

  const onKeyDown = (e: KeyboardEvent) => {
    const moves: Record<string, number> = {
      ArrowRight: active + 1,
      ArrowLeft: active - 1,
      Home: 0,
      End: tabs.length - 1,
    };
    const target = moves[e.key];
    if (target === undefined) return;
    e.preventDefault();
    const next = (target + tabs.length) % tabs.length;
    setSelected(next);
    listRef.current?.querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
  };

  return (
    <div className="sheet-tabs">
      <div role="tablist" className="sheet-tablist" ref={listRef} onKeyDown={onKeyDown}>
        {tabs.map((tab, i) => (
          <button
            key={i}
            type="button"
            role="tab"
            id={`${id}-tab-${i}`}
            aria-selected={i === active}
            aria-controls={`${id}-panel`}
            tabIndex={i === active ? 0 : -1}
            onClick={() => {
              setSelected(i);
            }}
          >
            {resolveLabel(tab.label, lang)}
          </button>
        ))}
      </div>
      {/* Only the open tab is mounted: a big sheet costs only what is on screen. */}
      <div
        role="tabpanel"
        id={`${id}-panel`}
        aria-labelledby={`${id}-tab-${active}`}
        className="sheet-tabpanel"
      >
        <Nodes key={active} nodes={current.children} scope={scope} base={base} />
      </div>
    </div>
  );
}

interface FieldProps {
  node: FieldNode;
  value: unknown;
  /** Absolute dotted path; a string so the memo comparison sees it as unchanged. */
  path: string;
}

/** Re-renders only when its own value changes, so typing in one field leaves the rest alone. */
const Field = memo(function Field({ node, value, path }: FieldProps) {
  const { lang, t, canEdit, schema, emit } = useActions();
  const fieldSchema = schemaAt(schema, path);
  const widget = widgetFor(node.widget, fieldSchema, value);
  const label = resolveLabel(node.label, lang) || fieldSchema?.title || path;

  if (!canEdit) {
    let shown: string;
    if (widget === 'checkbox') shown = value === true ? t('sheet.yes') : t('sheet.no');
    else if (value === undefined || value === null || value === '') shown = t('sheet.empty');
    else if (typeof value === 'string' || typeof value === 'number') shown = String(value);
    else shown = JSON.stringify(value);
    return (
      <div className={`sheet-field sheet-field-${widget}`}>
        <span className="sheet-field-label">{label}</span>
        <span className="sheet-field-value">{shown}</span>
      </div>
    );
  }

  const segments = splitPath(path);
  switch (widget) {
    case 'checkbox':
      return (
        <label className="sheet-field sheet-field-checkbox">
          <input
            type="checkbox"
            checked={value === true}
            onChange={(e) => {
              emit(segments, e.target.checked);
            }}
          />
          <span className="sheet-field-label">{label}</span>
        </label>
      );
    case 'select': {
      const options = fieldSchema?.enum ?? [];
      const known = options.some((o) => o === value);
      return (
        <label className="sheet-field sheet-field-select">
          <span className="sheet-field-label">{label}</span>
          <select
            value={known ? String(value) : ''}
            onChange={(e) => {
              const picked = options.find((o) => String(o) === e.target.value);
              if (picked !== undefined) emit(segments, picked);
            }}
          >
            {!known && (
              <option value="" disabled>
                {t('sheet.empty')}
              </option>
            )}
            {options.map((o) => (
              <option key={String(o)} value={String(o)}>
                {o}
              </option>
            ))}
          </select>
        </label>
      );
    }
    default:
      return (
        <TextualField
          widget={widget}
          label={label}
          value={value}
          schema={fieldSchema}
          onCommit={(next) => {
            emit(segments, next);
          }}
        />
      );
  }
});

/** Number, text and textarea: a local draft, emitted on blur or Enter; Escape reverts. */
function TextualField({
  widget,
  label,
  value,
  schema,
  onCommit,
}: {
  widget: 'number' | 'text' | 'textarea';
  label: string;
  value: unknown;
  schema: JsonSchema | undefined;
  onCommit: (value: string | number) => void;
}) {
  const { t } = useActions();
  // The label names the input only, not the −/+ buttons next to it.
  const inputId = useId();
  const [draft, setDraft] = useState<string | null>(null);
  const current = typeof value === 'string' || typeof value === 'number' ? String(value) : '';

  /** −/+ next to a number: the stored value (or 0) moved by one, clamped by the schema. */
  const step = (delta: number) => {
    setDraft(null);
    const n = parseInteger(String((typeof value === 'number' ? value : 0) + delta), schema);
    if (n !== null && n !== value) onCommit(n);
  };

  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    if (widget === 'number') {
      const n = parseInteger(draft, schema);
      if (n !== null && n !== value) onCommit(n);
    } else {
      const text = clampText(draft, schema);
      if (text !== value) onCommit(text);
    }
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') setDraft(null);
    // In a textarea Enter is a new line; Ctrl+Enter commits.
    if (e.key === 'Enter' && (widget !== 'textarea' || e.ctrlKey || e.metaKey)) commit();
  };
  const common = {
    id: inputId,
    value: draft ?? current,
    onChange: (e: { target: { value: string } }) => {
      setDraft(e.target.value);
    },
    onBlur: commit,
    onKeyDown,
  };

  return (
    <div className={`sheet-field sheet-field-${widget}`}>
      <label className="sheet-field-label" htmlFor={inputId}>
        {label}
      </label>
      {widget === 'textarea' ? (
        <textarea {...common} maxLength={schema?.maxLength} rows={4} />
      ) : widget === 'number' ? (
        <span className="sheet-number">
          <button
            type="button"
            className="sheet-step"
            tabIndex={-1}
            aria-label={t('sheet.decrease', { label })}
            title={t('sheet.decrease', { label })}
            onClick={() => {
              step(-1);
            }}
          >
            −
          </button>
          <input
            {...common}
            type="text"
            inputMode="numeric"
            aria-invalid={draft !== null && parseInteger(draft, schema) === null}
          />
          <button
            type="button"
            className="sheet-step"
            tabIndex={-1}
            aria-label={t('sheet.increase', { label })}
            title={t('sheet.increase', { label })}
            onClick={() => {
              step(1);
            }}
          >
            +
          </button>
        </span>
      ) : (
        <input {...common} type="text" maxLength={schema?.maxLength} />
      )}
    </div>
  );
}

function List({ node, value, path }: { node: ListNode; value: unknown; path: string[] }) {
  const { lang, t, canEdit, schema, emit } = useActions();
  const rows = Array.isArray(value) ? (value as unknown[]) : [];
  const listSchema = schemaAt(schema, path);
  const label = resolveLabel(node.label, lang);
  const full = listSchema?.maxItems !== undefined && rows.length >= listSchema.maxItems;

  return (
    <section className="sheet-list" aria-label={label}>
      <h3 className="sheet-section-title">{label}</h3>
      {rows.length === 0 && <p className="muted">{t('sheet.noRows')}</p>}
      {rows.map((row, i) => {
        const n = i + 1;
        return (
          <div
            key={i}
            role="group"
            className="sheet-list-row"
            aria-label={t('sheet.row', { label, n })}
          >
            <Nodes nodes={node.item} scope={row} base={[...path, String(i)]} />
            {canEdit && (
              <button
                type="button"
                className="sheet-list-remove"
                aria-label={t('sheet.removeRow', { label, n })}
                title={t('sheet.removeRow', { label, n })}
                onClick={() => {
                  emit(
                    path,
                    rows.filter((_, j) => j !== i),
                  );
                }}
              >
                ×
              </button>
            )}
          </div>
        );
      })}
      {canEdit && (
        <button
          type="button"
          className="sheet-list-add"
          disabled={full}
          onClick={() => {
            emit(path, [...rows, defaultValue(listSchema?.items)]);
          }}
        >
          {t('sheet.addRow', { label })}
        </button>
      )}
    </section>
  );
}

function Computed({
  name,
  label,
  format,
}: {
  name: string;
  label: Label;
  format: 'plain' | 'signed' | undefined;
}) {
  const { lang, t } = useActions();
  const { computed } = useContext(ValuesContext);
  const value = computed[name];
  const text = resolveLabel(label, lang);
  return (
    <div className="sheet-computed">
      <span className="sheet-field-label">{text}</span>
      <output className="sheet-computed-value" aria-label={text}>
        {value === undefined ? t('sheet.empty') : formatComputed(value, format)}
      </output>
    </div>
  );
}

function RollButton({ rollId, label }: { rollId: string; label: Label | undefined }) {
  const { lang, t, roll } = useActions();
  const { rolls, computed, data } = useContext(ValuesContext);
  const known = rolls.find((r) => r.id === rollId);
  const own = resolveLabel(known?.label, lang) || rollId;
  // The layout may show a short caption ("Roll"); the accessible name stays specific.
  const text = resolveLabel(label, lang) || own;
  return (
    <button
      type="button"
      className="sheet-roll"
      disabled={known === undefined}
      aria-label={t('sheet.roll', { label: known ? own : text })}
      title={known?.dice ?? t('sheet.missing', { id: rollId })}
      onClick={() => {
        roll(rollId);
      }}
    >
      <Icon name="dice" size={14} />
      <span className="sheet-roll-text">{text}</span>
      {known && <span className="sheet-roll-dice">{resolveDice(known.dice, computed, data)}</span>}
    </button>
  );
}

function Resource({ id }: { id: string }) {
  const { lang, t } = useActions();
  const { resources } = useContext(ValuesContext);
  const resource = resources.find((r) => r.id === id);
  if (resource === undefined) {
    return <div className="sheet-unsupported">{t('sheet.missing', { id })}</div>;
  }
  const label = resolveLabel(resource.label, lang);
  return (
    <div className="sheet-resource">
      <span className="sheet-field-label">{label}</span>
      <meter
        aria-label={label}
        min={0}
        max={Math.max(resource.max, 1)}
        low={Math.max(resource.max, 1) * 0.25}
        value={resource.value}
      />
      <span className="sheet-resource-value">
        {resource.value} / {resource.max}
      </span>
    </div>
  );
}
