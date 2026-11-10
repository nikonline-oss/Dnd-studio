import { useTranslation } from 'react-i18next';
import { useSubmitIntent } from '@/shared/game/useGame';
import type { CampaignMeta, TokenDto, TokenField, TokenInspectorPolicy } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';

/** Inspector rows in the order the server draws them (ADR-0061). */
const FIELDS: TokenField[] = [
  'name',
  'size',
  'disposition',
  'character',
  'conditions',
  'effects',
  'position',
];

/** Rows open when the campaign never changed the policy. */
const BASE_FIELDS: TokenField[] = ['name', 'size', 'character', 'conditions'];

type Choice = 'inherit' | 'open' | 'close';

/** `reveal` and `conceal` are optional on the wire, so read them through this. */
type Policy = { reveal: TokenField[]; conceal: TokenField[] };

const CHOICES: Choice[] = ['inherit', 'open', 'close'];

function read(policy: TokenInspectorPolicy | null | undefined): Policy {
  return { reveal: policy?.reveal ?? [], conceal: policy?.conceal ?? [] };
}

function campaignPolicy(campaign: CampaignMeta): Policy {
  return campaign.token_inspector
    ? read(campaign.token_inspector)
    : { reveal: BASE_FIELDS, conceal: [] };
}

function isOpen(policy: Policy, field: TokenField): boolean {
  return policy.reveal.includes(field) && !policy.conceal.includes(field);
}

/**
 * What players read about a token (ADR-0061). The campaign default applies to every token; the
 * per-token override below it opens or closes single rows for this token only.
 */
export function TokenInspectorPolicyForm({
  token,
  campaign,
  effectReveals,
}: {
  token: TokenDto;
  campaign: CampaignMeta;
  /** Rows active effects currently open (ADR-0061); they win over everything below. */
  effectReveals: TokenField[];
}) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const base = campaignPolicy(campaign);
  const override = read(token.inspector);
  // Distinct from the token form's own labels: both panels sit on the same screen.
  const label = (field: TokenField) =>
    `${t('inspector.policy.fieldLabel')} ${t(`inspector.policy.fields.${field}`)}`;

  const saveCampaign = (reveal: TokenField[]) => {
    submit.mutate({
      intent: {
        kind: 'update_campaign_meta',
        name: null,
        description: null,
        token_inspector: { reveal, conceal: [] },
      },
      expectedVersion: campaign.version,
    });
  };

  const toggleCampaign = (field: TokenField, open: boolean) => {
    const next = new Set(base.reveal.filter((f) => !base.conceal.includes(f)));
    if (open) next.add(field);
    else next.delete(field);
    saveCampaign(FIELDS.filter((f) => next.has(f)));
  };

  const choice = (field: TokenField): Choice => {
    if (override.reveal.includes(field)) return 'open';
    if (override.conceal.includes(field)) return 'close';
    return 'inherit';
  };

  const choose = (field: TokenField, value: Choice) => {
    const reveal = override.reveal.filter((f) => f !== field);
    const conceal = override.conceal.filter((f) => f !== field);
    if (value === 'open') reveal.push(field);
    if (value === 'close') conceal.push(field);
    submit.mutate({
      intent: {
        kind: 'update_token',
        id: token.id,
        name: null,
        rotation: null,
        scale: null,
        layer: null,
        visible: null,
        disposition: null,
        // An empty override is how the token follows the campaign again.
        inspector: { reveal, conceal },
      },
      expectedVersion: token.version,
    });
  };

  return (
    <div className="inspector-policy">
      <h3 className="section-title">
        <Icon name="eye" />
        {t('inspector.policy.title')}
      </h3>
      <p className="muted">{t('inspector.policy.hint')}</p>
      <ul className="policy-campaign">
        {FIELDS.map((field) => (
          <li key={field}>
            <label>
              <input
                type="checkbox"
                aria-label={label(field)}
                checked={isOpen(base, field)}
                onChange={(e) => {
                  toggleCampaign(field, e.currentTarget.checked);
                }}
              />
              {t(`inspector.policy.fields.${field}`)}
            </label>
          </li>
        ))}
      </ul>
      {effectReveals.length > 0 && (
        <p className="policy-active">
          {t('inspector.policy.effectHint', {
            fields: effectReveals.map((field) => t(`inspector.policy.fields.${field}`)).join(', '),
          })}
        </p>
      )}
      <p className="muted">{t('inspector.policy.tokenHint', { name: token.name })}</p>
      <ul className="policy-token">
        {FIELDS.map((field) => (
          <li key={field}>
            <label htmlFor={`policy-${field}`}>{t(`inspector.policy.fields.${field}`)}</label>
            <select
              id={`policy-${field}`}
              aria-label={label(field)}
              value={choice(field)}
              onChange={(e) => {
                choose(field, e.currentTarget.value as Choice);
              }}
            >
              {CHOICES.map((value) => (
                <option key={value} value={value}>
                  {t(`inspector.policy.choice.${value}`)}
                </option>
              ))}
            </select>
          </li>
        ))}
      </ul>
      <ErrorText error={submit.error} />
    </div>
  );
}
