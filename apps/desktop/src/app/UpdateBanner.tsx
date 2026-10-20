import { useMutation } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands, events, unwrap, type UpdateAvailable } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';

/**
 * Offers an update found by the background check (ADR-0013). Installing saves the campaign,
 * installs the signed update and restarts; nothing happens without the user's click.
 */
export function UpdateBanner() {
  const { t } = useTranslation();
  const [update, setUpdate] = useState<UpdateAvailable | null>(null);
  const install = useMutation({ mutationFn: () => unwrap(commands.updateInstall()) });

  useEffect(() => {
    const unlisten = events.updateAvailable.listen(({ payload }) => {
      setUpdate(payload);
    });
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);

  if (!update) return null;
  return (
    <div role="status" className="update-banner">
      <span>{t('update.available', { version: update.version })}</span>
      <button
        type="button"
        disabled={install.isPending}
        onClick={() => {
          install.mutate();
        }}
      >
        {install.isPending ? t('update.installing') : t('update.install')}
      </button>
      <button
        type="button"
        disabled={install.isPending}
        onClick={() => {
          setUpdate(null);
        }}
      >
        {t('update.later')}
      </button>
      <ErrorText error={install.error} />
    </div>
  );
}
