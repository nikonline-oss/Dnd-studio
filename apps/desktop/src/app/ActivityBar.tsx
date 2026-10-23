import { useTranslation } from 'react-i18next';
import type { SideView } from '@/shared/settings/settings';
import { Icon, type IconName } from '@/shared/ui/Icon';

export interface ActivityView {
  id: SideView;
  icon: IconName;
  /** A count shown on the icon (e.g. characters waiting for approval); hidden when 0. */
  badge?: number;
}

interface Props {
  views: ActivityView[];
  /** The view shown in the side panel; `null` while the panel is collapsed. */
  active: SideView | null;
  onSelect: (view: SideView) => void;
}

/**
 * The narrow icon strip on the left, as in VS Code: an icon opens its view in the side panel;
 * the active icon collapses the panel again.
 */
export function ActivityBar({ views, active, onSelect }: Props) {
  const { t } = useTranslation();
  return (
    <nav className="activity-bar" aria-label={t('activity.label')}>
      {views.map((view, n) => {
        const label = t(`activity.${view.id}`);
        const shortcut = `Alt+${String(n + 1)}`;
        return (
          <button
            key={view.id}
            type="button"
            className="activity-item"
            aria-pressed={active === view.id}
            aria-label={label}
            title={`${label} (${shortcut})`}
            onClick={() => {
              onSelect(view.id);
            }}
          >
            <Icon name={view.icon} size={22} />
            {view.badge !== undefined && view.badge > 0 && (
              <span
                className="activity-badge"
                aria-label={t('activity.badge', { count: view.badge })}
              >
                {view.badge}
              </span>
            )}
          </button>
        );
      })}
    </nav>
  );
}
