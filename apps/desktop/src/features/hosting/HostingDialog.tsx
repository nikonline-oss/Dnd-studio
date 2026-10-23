import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaignActions } from '@/shared/game/useCampaignActions';
import { remoteKey } from '@/shared/game/useRemote';
import { ipc, type HostedSession } from '@/shared/ipc/client';
import { useDialogs } from '@/shared/state/dialogs';
import { Dialog } from '@/shared/ui/Dialog';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { useToast } from '@/shared/ui/Toast';

const LAST_SERVER = 'dndstudio.lastServer';

function lastServer(): string {
  try {
    return localStorage.getItem(LAST_SERVER) ?? '';
  } catch {
    return '';
  }
}

function rememberServer(server: string) {
  try {
    localStorage.setItem(LAST_SERVER, server);
  } catch {
    // Not remembering is fine.
  }
}

/**
 * Invite players: uploads the open campaign to a session server (ADR-0022) and shows the access
 * code and the invite link to pass on; then the GM joins the session.
 */
export function HostingDialog() {
  const { t } = useTranslation();
  const close = useDialogs((s) => s.close);
  const [server, setServer] = useState(lastServer);
  const [hostToken, setHostToken] = useState('');
  const host = useMutation({
    mutationFn: () => ipc.remoteHost(server.trim(), hostToken),
    onSuccess: () => {
      rememberServer(server.trim());
    },
  });

  return (
    <Dialog title={t('session.hostTitle')} closeLabel={t('session.close')} onClose={close}>
      {host.data ? (
        <Hosted session={host.data} />
      ) : (
        <form
          className="dialog-form"
          onSubmit={(e) => {
            e.preventDefault();
            host.mutate();
          }}
        >
          <p className="muted">{t('session.hostIntro')}</p>
          <label>
            {t('session.server')}
            <input
              value={server}
              placeholder="https://dnd.example.org"
              onChange={(e) => {
                setServer(e.target.value);
              }}
            />
          </label>
          <label>
            {t('session.hostToken')}
            <input
              type="password"
              value={hostToken}
              autoComplete="off"
              onChange={(e) => {
                setHostToken(e.target.value);
              }}
            />
            <small className="muted">{t('session.hostTokenHint')}</small>
          </label>
          <button
            type="submit"
            className="btn-primary"
            disabled={host.isPending || server.trim() === ''}
          >
            {host.isPending ? t('session.uploading') : t('session.upload')}
          </button>
          <ErrorText error={host.error} />
        </form>
      )}
    </Dialog>
  );
}

function Hosted({ session }: { session: HostedSession }) {
  const { t } = useTranslation();
  const close = useDialogs((s) => s.close);
  const { onOpened } = useCampaignActions();
  const queryClient = useQueryClient();
  const showToast = useToast((s) => s.show);
  const join = useMutation({
    mutationFn: () => ipc.remoteConnect(session.server, session.session_id),
    onSuccess: (info) => {
      onOpened(info);
      void queryClient.invalidateQueries({ queryKey: remoteKey });
      close();
    },
  });
  const copy = (text: string) => {
    void navigator.clipboard
      .writeText(text)
      .then(() => {
        showToast(t('session.copied'));
      })
      .catch(() => undefined);
  };

  return (
    <div className="dialog-form">
      <p>{t('session.hosted', { name: session.name })}</p>
      <div className="secret-row">
        <span className="muted">{t('session.accessCode')}</span>
        <code className="access-code" aria-label={t('session.accessCode')}>
          {session.access_code}
        </code>
        <button
          type="button"
          onClick={() => {
            copy(session.access_code);
          }}
        >
          {t('session.copy')}
        </button>
      </div>
      <div className="secret-row">
        <span className="muted">{t('session.invite')}</span>
        <code className="invite-link" aria-label={t('session.invite')}>
          {session.invite_url}
        </code>
        <button
          type="button"
          onClick={() => {
            copy(session.invite_url);
          }}
        >
          {t('session.copy')}
        </button>
      </div>
      <p className="notice">
        <span>
          <Icon name="cloud" /> {t('session.hostedHint')}
        </span>
      </p>
      <button
        type="button"
        className="btn-primary"
        disabled={join.isPending}
        onClick={() => {
          join.mutate();
        }}
      >
        {t('session.joinAsGm')}
      </button>
      <ErrorText error={join.error} />
    </div>
  );
}
