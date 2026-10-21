import { useEffect, useRef, useState, type SyntheticEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { commands, unwrap, type ChatMessageDto, type DiceRollDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';

export type ChatInput =
  | { kind: 'text'; body: string }
  | { kind: 'roll'; expression: string; hidden: boolean }
  | { kind: 'empty' };

/** `/r` or `/roll` rolls publicly, `/gr` or `/gmroll` rolls hidden; anything else is text. */
export function parseChatInput(input: string): ChatInput {
  const text = input.trim();
  if (text === '') return { kind: 'empty' };
  const match = /^\/(r|roll|gr|gmroll)(?:\s+(.*))?$/is.exec(text);
  if (!match) return { kind: 'text', body: text };
  const command = (match[1] ?? '').toLowerCase();
  const expression = (match[2] ?? '').trim();
  if (expression === '') return { kind: 'empty' };
  return { kind: 'roll', expression, hidden: command === 'gr' || command === 'gmroll' };
}

const PAGE = 50;

/** Chat with dice rolls (bottom panel). */
export function ChatPanel() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const { data: snapshot } = useSnapshot();
  const submit = useSubmitIntent();
  const characterId = useSelection((s) => s.characterId);
  const [input, setInput] = useState('');
  const [older, setOlder] = useState<ChatMessageDto[]>([]);
  const [noMore, setNoMore] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const recent = snapshot?.chat ?? [];
  const messages = [...older, ...recent];
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const character = snapshot?.characters.find((c) => c.id === characterId);
  // Field references (@str_mod) read the selected character's sheet if the user may use it.
  const rollAs =
    character && (isGm || character.owner === campaign?.user_id) ? character : undefined;

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: 'end' });
  }, [recent.length]);

  const loadOlder = async () => {
    const first = messages[0];
    if (!first) return;
    const page = await unwrap(commands.chatHistory(first.id, PAGE));
    if (page.length < PAGE) setNoMore(true);
    setOlder((prev) => [...page, ...prev]);
  };

  const send = (e: SyntheticEvent) => {
    e.preventDefault();
    setLocalError(null);
    const parsed = parseChatInput(input);
    if (parsed.kind === 'empty') return;
    if (parsed.kind === 'roll' && parsed.hidden && !isGm) {
      setLocalError(t('chat.hiddenGmOnly'));
      return;
    }
    submit.mutate(
      {
        intent:
          parsed.kind === 'text'
            ? { kind: 'send_message', id: uuidv7(), body: parsed.body }
            : {
                kind: 'roll_dice',
                id: uuidv7(),
                expression: parsed.expression,
                hidden: parsed.hidden,
                character_id: rollAs?.id ?? null,
              },
      },
      {
        onSuccess: () => {
          setInput('');
        },
      },
    );
  };

  return (
    <section className="chat-panel" aria-label={t('panels.chat')}>
      <ol className="chat-list">
        {messages.length > 0 && !noMore && (
          <li className="chat-more">
            <button type="button" onClick={() => void loadOlder()}>
              {t('chat.loadOlder')}
            </button>
          </li>
        )}
        {messages.map((m) => (
          <ChatLine key={m.id} message={m} />
        ))}
        <div ref={bottom} />
      </ol>
      <form className="chat-input" onSubmit={send}>
        <input
          aria-label={t('chat.input')}
          placeholder={t('chat.placeholder')}
          value={input}
          maxLength={4000}
          onChange={(e) => {
            setInput(e.target.value);
          }}
        />
        <button type="submit" disabled={submit.isPending}>
          {t('chat.send')}
        </button>
      </form>
      {rollAs && <p className="muted">{t('chat.rollingAs', { name: rollAs.name })}</p>}
      {localError && (
        <p role="alert" className="error-text">
          {localError}
        </p>
      )}
      <ErrorText error={submit.error} />
    </section>
  );
}

function ChatLine({ message: m }: { message: ChatMessageDto }) {
  const { t } = useTranslation();
  const time = m.at_ms === null ? '' : new Date(m.at_ms).toLocaleTimeString();
  if (m.kind === 'system' && m.system) {
    return (
      <li className="chat-line" data-kind="system">
        <time>{time}</time> <em>{t(`chat.system.${m.system.code}`, { name: m.system.name })}</em>
      </li>
    );
  }
  return (
    <li className="chat-line" data-kind={m.kind}>
      <time>{time}</time> <strong>{m.sender_name || t('chat.unknownSender')}</strong>
      {m.visibility === 'gm' && <span className="badge">{t('chat.hidden')}</span>}{' '}
      {m.roll ? <RollView roll={m.roll} typed={m.body} /> : <span>{m.body}</span>}
    </li>
  );
}

function RollView({ roll, typed }: { roll: DiceRollDto; typed: string }) {
  const { t } = useTranslation();
  return (
    <span className="roll" title={typed}>
      <span className="roll-expr">{roll.expression}</span>{' '}
      {roll.terms.map((term, n) => (
        <span key={n} className="roll-term">
          {term.negative ? '− ' : n > 0 ? '+ ' : ''}
          {term.dice.length > 0 ? (
            <>
              [
              {term.dice.map((d, i) => (
                <span key={i} className="die" data-kept={d.kept}>
                  {d.value}
                </span>
              ))}
              ]
            </>
          ) : (
            term.notation
          )}{' '}
        </span>
      ))}
      ={' '}
      <strong className="roll-total" aria-label={t('chat.total')}>
        {roll.total}
      </strong>
    </span>
  );
}
