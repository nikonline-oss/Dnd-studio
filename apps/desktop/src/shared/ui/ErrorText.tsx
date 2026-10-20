import { useTranslation } from 'react-i18next';
import { errorCode } from '@/shared/ipc/client';

/** Localized message for an IPC error or rejected intent. */
export function ErrorText({ error }: { error: unknown }) {
  const { t } = useTranslation();
  if (!error) return null;
  const code = errorCode(error);
  return (
    <p role="alert" className="error-text">
      {t(`errors.${code}`, { defaultValue: t('errors.unknown') })}
    </p>
  );
}
