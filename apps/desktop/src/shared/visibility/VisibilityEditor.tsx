import { useTranslation } from 'react-i18next';
import { useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import type { Label, VisibilityPolicy } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';

interface Leaf {
  path: string;
  label: string;
  /** The system schema's own answer; the switch starts here. */
  publicByDefault: boolean;
  /** Objects group their leaves; the group is drawn as a heading. */
  group?: string;
}

/**
 * What players may read of other people's characters (ADR-0063): one switch per sheet field, plus
 * the character's name and the token's picture. A switch that matches the system schema is stored
 * as no rule at all, so a plugin update's defaults keep working until the GM changes something.
 */
export function VisibilityEditor() {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const lang = i18n.resolvedLanguage ?? i18n.language;
  const campaign = snapshot?.campaign;
  const system = snapshot?.plugins.find((p) => p.id === snapshot.system_plugin)?.system;
  if (!campaign) return null;
  const policy: VisibilityPolicy = campaign.visibility ?? {};
  const rules = new Map((policy.sheet ?? []).map((rule) => [rule.path, rule.visible]));
  const leaves = schemaLeaves(system?.sheet_schema, lang);

  const save = (next: VisibilityPolicy) => {
    submit.mutate({
      intent: {
        kind: 'update_campaign_meta',
        name: null,
        description: null,
        token_inspector: null,
        item_states: null,
        visibility: next,
      },
      expectedVersion: campaign.version,
    });
  };

  const toggle = (leaf: Leaf, visible: boolean) => {
    const sheet = new Map(rules);
    if (visible === leaf.publicByDefault) sheet.delete(leaf.path);
    else sheet.set(leaf.path, visible);
    save({
      sheet: [...sheet].map(([path, value]) => ({ path, visible: value })),
      name: policy.name ?? null,
      icon: policy.icon ?? null,
    });
  };

  const identity = (key: 'name' | 'icon', visible: boolean) => {
    save({
      sheet: [...rules].map(([path, value]) => ({ path, visible: value })),
      name: key === 'name' ? visible : (policy.name ?? null),
      icon: key === 'icon' ? visible : (policy.icon ?? null),
    });
  };

  if (!system) {
    return <p className="muted">{t('visibility.noSystem')}</p>;
  }

  const groups = [...new Set(leaves.map((leaf) => leaf.group ?? ''))];

  return (
    <div className="visibility-editor">
      <p className="muted">{t('visibility.hint')}</p>
      <ul className="visibility-identity">
        <li>
          <label className="control-checkbox">
            <input
              type="checkbox"
              checked={policy.name ?? true}
              onChange={(e) => {
                identity('name', e.currentTarget.checked);
              }}
            />
            {t('visibility.name')}
          </label>
        </li>
        <li>
          <label className="control-checkbox">
            <input
              type="checkbox"
              checked={policy.icon ?? true}
              onChange={(e) => {
                identity('icon', e.currentTarget.checked);
              }}
            />
            {t('visibility.icon')}
          </label>
        </li>
      </ul>
      {groups.map((group) => (
        <section key={group} className="visibility-group">
          {group && <h4>{group}</h4>}
          <ul>
            {leaves
              .filter((leaf) => (leaf.group ?? '') === group)
              .map((leaf) => (
                <li key={leaf.path}>
                  <label className="control-checkbox">
                    <input
                      type="checkbox"
                      checked={rules.get(leaf.path) ?? leaf.publicByDefault}
                      onChange={(e) => {
                        toggle(leaf, e.currentTarget.checked);
                      }}
                    />
                    {leaf.label}
                  </label>
                </li>
              ))}
          </ul>
        </section>
      ))}
      <ErrorText error={submit.error} />
    </div>
  );
}

/** Every readable leaf of the sheet schema, with its dotted path and the plugin's own flag. */
function schemaLeaves(schema: unknown, lang: string): Leaf[] {
  const leaves: Leaf[] = [];
  const walk = (node: unknown, prefix: string, group: string | undefined) => {
    const object = node as
      | {
          type?: string;
          title?: Label;
          visible_to_players?: boolean;
          properties?: Record<string, unknown>;
        }
      | undefined;
    if (!object) return;
    const label = object.title ? labelText(object.title, lang) : prefix;
    if (object.properties) {
      for (const [name, child] of Object.entries(object.properties)) {
        walk(child, prefix ? `${prefix}.${name}` : name, group ?? label);
      }
      return;
    }
    if (!prefix) return;
    leaves.push({
      path: prefix,
      label: label || prefix,
      publicByDefault: object.visible_to_players === true,
      ...(group ? { group } : {}),
    });
  };
  walk(schema, '', undefined);
  return leaves;
}
