import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useCampaignActions } from '@/shared/game/useCampaignActions';
import { useCampaign } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { commands, errorCode, ipc, unwrap } from '@/shared/ipc/client';
import { useDialogs } from '@/shared/state/dialogs';
import { useSettings, type Settings } from '@/shared/settings/settings';
import { Icon } from '@/shared/ui/Icon';
import { Menu } from '@/shared/ui/Menu';
import { useToast } from '@/shared/ui/Toast';

export function MenuBar({ breadcrumbs }: { breadcrumbs: string[] }) {
  const { t } = useTranslation();
  const { open, close, importArchive, exportArchive } = useCampaignActions();
  const showToast = useToast((s) => s.show);
  const showDialog = useDialogs((s) => s.show);
  const remote = useRemote();
  const { data: campaign } = useCampaign();
  const stopSession = useMutation({
    mutationFn: ipc.remoteStop,
    onSuccess: () => {
      close.mutate();
    },
  });
  const failed = (error: unknown) => {
    showToast(t(`errors.${errorCode(error)}`, { defaultValue: t('errors.unknown') }), 'error');
  };
  const settings = useSettings();
  // A found update is announced by the UpdateBanner via the event; here we only report the rest.
  const checkUpdate = useMutation({ mutationFn: () => unwrap(commands.updateCheck()) });
  const updateMessage = checkUpdate.isError
    ? t(`errors.${errorCode(checkUpdate.error)}`, { defaultValue: t('errors.unknown') })
    : checkUpdate.data === null
      ? t('update.upToDate')
      : null;

  const option = <K extends keyof Settings>(key: K, value: Settings[K], label: string) => ({
    label,
    checked: settings[key] === value,
    onSelect: () => {
      settings.update({ [key]: value });
    },
  });

  return (
    <header className="menubar">
      <span className="brand" aria-hidden="true">
        <Icon name="dice" size={16} />
      </span>
      <nav aria-label={t('menu.label')} className="menus">
        <Menu
          label={t('menu.file')}
          items={[
            {
              label: t('campaign.open'),
              onSelect: () => {
                open.mutate();
              },
            },
            {
              label: t('campaign.import'),
              onSelect: () => {
                importArchive.mutate(undefined, { onError: failed });
              },
            },
            {
              label: t('campaign.export'),
              // A session's campaign lives on the server (bringing it home: stage 4).
              disabled: exportArchive.isPending || remote !== null,
              onSelect: () => {
                exportArchive.mutate(undefined, {
                  onSuccess: (path) => {
                    if (path !== null) showToast(t('campaign.exported'));
                  },
                  onError: failed,
                });
              },
            },
            'separator',
            {
              label: t('campaign.close'),
              onSelect: () => {
                close.mutate();
              },
            },
          ]}
        />
        <Menu
          label={t('menu.session')}
          items={[
            {
              label: t('session.host'),
              disabled: remote !== null || campaign?.role !== 'gm',
              onSelect: () => {
                showDialog('hosting');
              },
            },
            {
              label: t('session.connect'),
              onSelect: () => {
                showDialog('connect');
              },
            },
            'separator',
            {
              label: t('session.stop'),
              disabled: remote?.role !== 'gm' || stopSession.isPending,
              onSelect: () => {
                stopSession.mutate(undefined, { onError: failed });
              },
            },
            {
              label: t('session.leave'),
              disabled: remote === null,
              onSelect: () => {
                close.mutate();
              },
            },
          ]}
        />
        <Menu
          label={t('menu.view')}
          items={[
            option('layoutPreset', 'gm', t('layout.gm')),
            option('layoutPreset', 'player', t('layout.player')),
            'separator',
            option('theme', 'system', t('theme.system')),
            option('theme', 'light', t('theme.light')),
            option('theme', 'dark', t('theme.dark')),
            'separator',
            option('language', 'system', t('language.system')),
            option('language', 'ru', 'Русский'),
            option('language', 'en', 'English'),
          ]}
        />
        <Menu
          label={t('menu.help')}
          items={[
            {
              label: t('update.check'),
              disabled: checkUpdate.isPending,
              onSelect: () => {
                checkUpdate.mutate();
              },
            },
          ]}
        />
      </nav>
      <ol className="breadcrumbs" aria-label={t('menu.breadcrumbs')}>
        {breadcrumbs.map((crumb, n) => (
          <li key={`${n}-${crumb}`}>{crumb}</li>
        ))}
      </ol>
      {updateMessage && (
        <span role="status" className="muted">
          {updateMessage}
        </span>
      )}
    </header>
  );
}
