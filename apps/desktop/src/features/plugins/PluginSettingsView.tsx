import { useId } from 'react';
import { useSettings } from '@/shared/settings/settings';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { useRemote } from '@/shared/game/useRemote';
import { ErrorText } from '@/shared/ui/ErrorText';

export function PluginSettingsView({ pluginId }: { pluginId: string }) {
  const { t, i18n } = useTranslation();
  const settingsId = useId();
  const effects = useSettings((s) => s.pluginEffects);
  const opacity = useSettings((s) => s.pluginEffectOpacity);
  const update = useSettings((s) => s.update);
  const plugin = useSnapshotSelect((s) => s.plugins.find((p) => p.id === pluginId));
  const { data: campaign } = useCampaign();
  const remote = useRemote();
  const submit = useSubmitIntent();
  const canEdit =
    (campaign?.role === 'gm' || campaign?.role === 'co_gm') &&
    (remote === null || remote.status.state === 'online');
  if (!plugin) return null;
  const lang = i18n.resolvedLanguage ?? i18n.language;
  return (
    <section className="plugin-settings" aria-label={t('plugins.ruleSettings')}>
      <header className="control-page-header">
        <h2>{labelText(plugin.name, lang)}</h2>
        <p className="muted">
          {plugin.version} · {plugin.license}
        </p>
      </header>
      <section className="control-section">
        <h3>{t('plugins.ruleSettings')}</h3>
        <p className="muted">{t('plugins.campaignRulesHint')}</p>
        {!canEdit && <p>{t('plugins.rulesReadOnly')}</p>}
        {(plugin.system?.features ?? []).length === 0 && <p>{t('plugins.noRuleSettings')}</p>}
        <div className="plugin-features">
          {(plugin.system?.features ?? []).map((feature) => (
            <div className="plugin-feature" key={feature.id}>
              <label>
                <input
                  type="checkbox"
                  aria-labelledby={`${settingsId}-${feature.id}-label`}
                  aria-describedby={`${settingsId}-${feature.id}-description`}
                  checked={plugin.feature_settings?.[feature.id] ?? feature.default_enabled}
                  disabled={!canEdit || submit.isPending}
                  onChange={(e) => {
                    submit.mutate({
                      intent: {
                        kind: 'configure_plugin_feature',
                        id: plugin.id,
                        feature: feature.id,
                        enabled: e.target.checked,
                      },
                    });
                  }}
                />
                <span>
                  <strong id={`${settingsId}-${feature.id}-label`}>
                    {labelText(feature.label, lang)}
                  </strong>
                  <span
                    id={`${settingsId}-${feature.id}-description`}
                    className="muted plugin-feature-description"
                  >
                    {labelText(feature.description, lang)}
                  </span>
                </span>
              </label>
            </div>
          ))}
        </div>
      </section>
      <section className="control-section plugin-appearance">
        <h3>{t('plugins.localAppearance')}</h3>
        <label className="control-checkbox">
          <input
            type="checkbox"
            checked={effects}
            onChange={(e) => {
              update({ pluginEffects: e.target.checked });
            }}
          />
          {t('plugins.showEffects')}
        </label>
        <label className="control-range">
          <span>
            {t('plugins.effectOpacity')} <output>{Math.round(opacity * 100)}%</output>
          </span>
          <input
            type="range"
            aria-label={t('plugins.effectOpacity')}
            min="0"
            max="1"
            step="0.05"
            value={opacity}
            onChange={(e) => {
              update({ pluginEffectOpacity: Number(e.target.value) });
            }}
          />
        </label>
      </section>
      <ErrorText error={submit.error} />
    </section>
  );
}
