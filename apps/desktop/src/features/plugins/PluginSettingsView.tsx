import { useSettings } from '@/shared/settings/settings';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import { useRemote } from '@/shared/game/useRemote';
import { ErrorText } from '@/shared/ui/ErrorText';

export function PluginSettingsView({ pluginId }: { pluginId: string }) {
  const { t, i18n } = useTranslation();
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
      <h2>{labelText(plugin.name, lang)}</h2>
      <p>
        {plugin.version} · {plugin.license}
      </p>
      <h3>{t('plugins.ruleSettings')}</h3>
      <p className="muted">{t('plugins.campaignRulesHint')}</p>
      {!canEdit && <p>{t('plugins.rulesReadOnly')}</p>}
      {(plugin.system?.features ?? []).length === 0 && <p>{t('plugins.noRuleSettings')}</p>}
      {(plugin.system?.features ?? []).map((feature) => (
        <div className="plugin-feature" key={feature.id}>
          <label>
            <input
              type="checkbox"
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
            {labelText(feature.label, lang)}
          </label>
          <p className="muted">{labelText(feature.description, lang)}</p>
        </div>
      ))}
      <h3>{t('plugins.localAppearance')}</h3>
      <label>
        <input
          type="checkbox"
          checked={effects}
          onChange={(e) => {
            update({ pluginEffects: e.target.checked });
          }}
        />
        {t('plugins.showEffects')}
      </label>
      <label>
        {t('plugins.effectOpacity')}
        <input
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={opacity}
          onChange={(e) => {
            update({ pluginEffectOpacity: Number(e.target.value) });
          }}
        />
      </label>
      <ErrorText error={submit.error} />
    </section>
  );
}
