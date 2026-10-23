import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaignActions } from '@/shared/game/useCampaignActions';
import { remoteKey } from '@/shared/game/useRemote';
import { ipc, type RemoteEntry } from '@/shared/ipc/client';
import { useDialogs } from '@/shared/state/dialogs';
import { Dialog } from '@/shared/ui/Dialog';
import { ErrorText } from '@/shared/ui/ErrorText';
import { IconButton } from '@/shared/ui/IconButton';

const sessionsKey = ['remoteSessions'] as const;

/** Join a session with an invite link and an access code, or reconnect to a saved one. */
export function ConnectDialog() {
  const { t } = useTranslation();
  const close = useDialogs((s) => s.close);
  const { onOpened } = useCampaignActions();
  const queryClient = useQueryClient();
  const [invite, setInvite] = useState('');
  const [code, setCode] = useState('');
  const [name, setName] = useState('');
  const saved = useQuery({ queryKey: sessionsKey, queryFn: ipc.remoteSessions });

  const connected = (info: Awaited<ReturnType<typeof ipc.remoteJoin>>) => {
    onOpened(info);
    void queryClient.invalidateQueries({ queryKey: remoteKey });
    close();
  };
  const join = useMutation({
    mutationFn: () => ipc.remoteJoin(invite.trim(), code.trim(), name.trim()),
    onSuccess: connected,
  });
  const reconnect = useMutation({
    mutationFn: (entry: RemoteEntry) => ipc.remoteConnect(entry.server, entry.session_id),
    onSuccess: connected,
  });
  const forget = useMutation({
    mutationFn: (entry: RemoteEntry) => ipc.remoteForget(entry.server, entry.session_id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: sessionsKey });
    },
  });
  const busy = join.isPending || reconnect.isPending;

  return (
    <Dialog title={t('session.connectTitle')} closeLabel={t('session.close')} onClose={close}>
      <form
        className="dialog-form"
        onSubmit={(e) => {
          e.preventDefault();
          join.mutate();
        }}
      >
        <label>
          {t('session.invite')}
          <input
            value={invite}
            placeholder="https://dnd.example.org/api/sessions/…"
            onChange={(e) => {
              setInvite(e.target.value);
            }}
          />
        </label>
        <label>
          {t('session.accessCode')}
          <input
            value={code}
            autoComplete="off"
            placeholder="ABCDE-FGHJK"
            onChange={(e) => {
              setCode(e.target.value);
            }}
          />
        </label>
        <label>
          {t('session.yourName')}
          <input
            value={name}
            maxLength={200}
            onChange={(e) => {
              setName(e.target.value);
            }}
          />
        </label>
        <button
          type="submit"
          className="btn-primary"
          disabled={busy || invite.trim() === '' || code.trim() === '' || name.trim() === ''}
        >
          {t('session.join')}
        </button>
        <ErrorText error={join.error} />
      </form>

      {saved.data && saved.data.length > 0 && (
        <section className="saved-sessions" aria-label={t('session.saved')}>
          <h3>{t('session.saved')}</h3>
          <ul>
            {saved.data.map((entry) => (
              <li key={`${entry.server}/${entry.session_id}`} className="saved-session">
                <div className="saved-session-name">
                  <strong>{entry.session_name}</strong>
                  <span className="muted">
                    {entry.server}
                    {entry.role && ` · ${t(`roles.${entry.role}`)}`}
                    {entry.access_code && ` · ${t('session.accessCode')}: ${entry.access_code}`}
                  </span>
                </div>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => {
                    reconnect.mutate(entry);
                  }}
                >
                  {t('session.reconnect')}
                </button>
                <IconButton
                  icon="trash"
                  label={t('session.forget')}
                  onClick={() => {
                    forget.mutate(entry);
                  }}
                />
              </li>
            ))}
          </ul>
          <ErrorText error={reconnect.error ?? forget.error} />
        </section>
      )}
    </Dialog>
  );
}
