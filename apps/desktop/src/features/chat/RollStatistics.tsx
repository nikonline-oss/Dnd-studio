import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useCampaign } from '@/shared/game/useGame';
import { commands, unwrap } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';
import './rollStatistics.css';

/** Only server-authorized aggregates; never infer statistics from partial chat pages. */
export function RollStatistics() {
  const { t, i18n } = useTranslation();
  const { data: campaign } = useCampaign();
  const statistics = useQuery({
    queryKey: ['roll-statistics', campaign?.path, campaign?.user_id, campaign?.role],
    queryFn: () => unwrap(commands.rollStatistics()),
    enabled: !!campaign,
    refetchInterval: 10_000,
  });
  const format = new Intl.NumberFormat(i18n.resolvedLanguage ?? i18n.language, {
    maximumFractionDigits: 2,
  });
  return (
    <section className="roll-statistics" aria-label={t('chat.statistics.title')}>
      <div className="roll-statistics-heading">
        <p>{t('chat.statistics.scope')}</p>
        <button
          type="button"
          disabled={statistics.isFetching}
          onClick={() => {
            void statistics.refetch();
          }}
        >
          {t('chat.statistics.refresh')}
        </button>
      </div>
      <ErrorText error={statistics.error} />
      {statistics.isPending && <p role="status">{t('chat.statistics.loading')}</p>}
      {statistics.data?.length === 0 && <p>{t('chat.statistics.empty')}</p>}
      {!!statistics.data?.length && (
        <div className="roll-statistics-scroll">
          <table>
            <caption>{t('chat.statistics.title')}</caption>
            <thead>
              <tr>
                {['participant', 'rolls', 'average', 'minimum', 'maximum', 'ones', 'twenties'].map(
                  (key) => (
                    <th key={key} scope="col">
                      {t(`chat.statistics.${key}`)}
                    </th>
                  ),
                )}
              </tr>
            </thead>
            <tbody>
              {statistics.data.map((row) => (
                <tr key={row.user_id ?? `name:${row.name}`}>
                  <th scope="row">
                    {row.name || t('chat.unknownSender')}
                    {row.user_id === campaign?.user_id && <small>{t('chat.statistics.you')}</small>}
                  </th>
                  <td>{format.format(row.rolls)}</td>
                  <td>{format.format(row.average ?? 0)}</td>
                  <td>{format.format(row.minimum)}</td>
                  <td>{format.format(row.maximum)}</td>
                  <td>{format.format(row.natural_ones)}</td>
                  <td>{format.format(row.natural_twenties)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="roll-statistics-note">{t('chat.statistics.explanation')}</p>
    </section>
  );
}
