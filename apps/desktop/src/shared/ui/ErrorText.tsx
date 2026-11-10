import { useTranslation } from 'react-i18next';
import { errorCode } from '@/shared/ipc/client';

/** Localized message for an IPC error or rejected intent. */
export function ErrorText({
  error,
  onRetry,
  retrying = false,
}: {
  error: unknown;
  onRetry?: (() => void) | undefined;
  retrying?: boolean;
}) {
  const { t } = useTranslation();
  if (!error) return null;
  const code = errorCode(error);
  const hint = ['server_unreachable', 'connection_lost', 'timeout', 'offline'].includes(code)
    ? 'network'
    : code === 'invalid_address'
      ? 'address'
      : code === 'download_too_large'
        ? 'size'
        : ['signature_invalid', 'hash_mismatch', 'signature_missing'].includes(code)
          ? 'signature'
          : null;
  return (
    <div role="alert" className="error-notice">
      <p className="error-text">{t(`errors.${code}`, { defaultValue: t('errors.unknown') })}</p>
      {hint && <p className="muted">{t(`errorUi.${hint}`)}</p>}
      {onRetry && (
        <button type="button" disabled={retrying} onClick={onRetry}>
          {t('errorUi.retry')}
        </button>
      )}
      <details>
        <summary>{t('errorUi.details')}</summary>
        <span>{t('errorUi.code')}: </span>
        <code>{code}</code>
      </details>
    </div>
  );
}
