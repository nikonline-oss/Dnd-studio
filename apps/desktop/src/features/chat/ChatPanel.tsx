import { useMutation } from '@tanstack/react-query';
import type { TFunction } from 'i18next';
import { useEffect, useRef, useState, type SyntheticEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { labelText } from '@/shared/i18n/label';
import {
  commands,
  unwrap,
  type ChatMessageDto,
  type DiceRollDto,
  type SystemMessage,
  type SystemRulesDto,
} from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';

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
  const submit = useSubmitIntent();
  const characterId = useSelection((s) => s.characterId);
  const [input, setInput] = useState('');
  const [older, setOlder] = useState<ChatMessageDto[]>([]);
  const [noMore, setNoMore] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const recent = useSnapshotSelect((s) => s.chat) ?? [];
  const messages = [...new Map([...older, ...recent].map((m) => [m.id, m])).values()];
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const character = useSnapshotSelect((s) => s.characters.find((c) => c.id === characterId));
  // Field references (@str_mod) read the selected character's sheet if the user may use it.
  const rollAs =
    character && (isGm || character.owner === campaign?.user_id) ? character : undefined;

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: 'end' });
  }, [recent.length]);

  const history = useMutation({
    mutationFn: () => unwrap(commands.chatHistory(messages[0]?.id ?? null, PAGE)),
    onSuccess: (page) => {
      if (page.length < PAGE) setNoMore(true);
      setOlder((prev) => [...page, ...prev]);
    },
  });

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
                roll: null,
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
        {!noMore && (
          <li className="chat-more">
            <button
              type="button"
              disabled={history.isPending}
              onClick={() => {
                history.mutate();
              }}
            >
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
        <Icon name="dice" className="chat-input-icon" />
        <input
          aria-label={t('chat.input')}
          placeholder={t('chat.placeholder')}
          value={input}
          maxLength={4000}
          onChange={(e) => {
            setInput(e.target.value);
          }}
        />
        {rollAs && <span className="chat-as">{t('chat.rollingAs', { name: rollAs.name })}</span>}
        <button
          type="submit"
          className="icon-button btn-primary"
          aria-label={t('chat.send')}
          title={t('chat.send')}
          disabled={submit.isPending}
        >
          <Icon name="send" />
        </button>
      </form>
      {localError && (
        <p role="alert" className="error-text">
          {localError}
        </p>
      )}
      <ErrorText error={submit.error ?? history.error} />
    </section>
  );
}

/** The text of a system message; resources and conditions by their names in the system. */
function systemText(
  message: SystemMessage,
  t: TFunction,
  rules: SystemRulesDto | null | undefined,
  lang: string,
): string {
  switch (message.code) {
    case 'character_submitted':
    case 'character_approved':
    case 'character_returned':
      return t(`chat.system.${message.code}`, { name: message.name });
    case 'narration':
      return message.text;
    case 'resource_changed': {
      const def = rules?.resources.find((r) => r.id === message.resource);
      return t(message.delta < 0 ? 'chat.system.resource_lost' : 'chat.system.resource_gained', {
        name: message.name,
        resource: def ? labelText(def.label, lang) : message.resource,
        amount: Math.abs(message.delta),
      });
    }
    case 'condition_changed': {
      const def = rules?.conditions?.find((c) => c.id === message.condition);
      return t(message.added ? 'chat.system.condition_added' : 'chat.system.condition_removed', {
        name: message.name,
        condition: def ? labelText(def.label, lang) : message.condition,
      });
    }
    case 'item_given':
      return t('chat.system.item_given', { name: message.name, item: message.item });
  }
}

function ChatLine({ message: m }: { message: ChatMessageDto }) {
  const { t, i18n } = useTranslation();
  const rules = useSnapshotSelect(
    (s) => s.plugins.find((p) => p.id === s.system_plugin)?.system ?? null,
  );
  const time =
    m.at_ms === null
      ? ''
      : new Date(m.at_ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (m.kind === 'system' && m.system) {
    return (
      <li className="chat-line" data-kind="system" data-hidden={m.visibility === 'gm'}>
        <Icon name="sparkle" />
        <em>{systemText(m.system, t, rules, i18n.resolvedLanguage ?? i18n.language)}</em>
        <time>{time}</time>
      </li>
    );
  }
  const name = m.sender_name || t('chat.unknownSender');
  return (
    <li className="chat-line" data-kind={m.kind} data-hidden={m.visibility === 'gm'}>
      <span className="avatar" aria-hidden="true">
        {name.trim().charAt(0).toUpperCase()}
      </span>
      <div className="chat-body">
        <div className="chat-head">
          <strong>{name}</strong>
          {m.visibility === 'gm' && (
            <span className="badge" data-status="pending_approval">
              {t('chat.hidden')}
            </span>
          )}
          <time>{time}</time>
        </div>
        {m.roll ? <RollView roll={m.roll} typed={m.body} /> : <p className="chat-text">{m.body}</p>}
      </div>
    </li>
  );
}

function RollView({ roll, typed }: { roll: DiceRollDto; typed: string }) {
  const { t } = useTranslation();
  return (
    <div className="roll" title={typed}>
      <Icon name="dice" className="roll-icon" />
      <span className="roll-expr">{roll.expression}</span>
      <span className="roll-terms">
        {roll.terms.map((term, n) => (
          <span key={n} className="roll-term">
            {term.negative ? <span className="op">−</span> : n > 0 && <span className="op">+</span>}
            {term.dice.length > 0 ? (
              term.dice.map((d, i) => (
                <span key={i} className="die" data-kept={d.kept}>
                  {d.value}
                </span>
              ))
            ) : (
              <span className="roll-const">{term.notation}</span>
            )}
          </span>
        ))}
      </span>
      <strong className="roll-total" aria-label={t('chat.total')}>
        {roll.total}
      </strong>
    </div>
  );
}
