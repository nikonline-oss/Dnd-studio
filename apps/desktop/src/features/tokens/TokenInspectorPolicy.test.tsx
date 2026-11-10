import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TokenInspectorPolicyForm } from './TokenInspectorPolicy';
import type { CampaignMeta, TokenDto } from '@/shared/ipc/client';

const state = vi.hoisted(() => ({
  mutate: vi.fn(),
}));

vi.mock('@/shared/game/useGame', () => ({
  useSubmitIntent: () => ({ mutate: state.mutate, isPending: false, error: null }),
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const campaign: CampaignMeta = {
  name: 'C',
  description: '',
  token_inspector: { reveal: ['name', 'size', 'character', 'conditions'], conceal: [] },
  version: 7,
};

const token: TokenDto = {
  id: 't1',
  map_id: 'm1',
  character_id: null,
  asset_id: null,
  name: 'Knight',
  x: 160,
  y: 32,
  rotation: 0,
  scale: 1,
  layer: 'tokens',
  visible: true,
  version: 4,
};

const name = (field: string) => `inspector.policy.fieldLabel inspector.policy.fields.${field}`;

beforeEach(() => {
  state.mutate.mockClear();
});

describe('TokenInspectorPolicyForm', () => {
  it('shows the campaign rows that are open', () => {
    render(<TokenInspectorPolicyForm token={token} campaign={campaign} effectReveals={[]} />);
    const nameBox = screen.getByRole<HTMLInputElement>('checkbox', {
      name: name('name'),
    });
    const effectsBox = screen.getByRole<HTMLInputElement>('checkbox', {
      name: name('effects'),
    });
    expect(nameBox.checked).toBe(true);
    expect(effectsBox.checked).toBe(false);
  });

  it('opens a campaign field with the campaign version', () => {
    render(<TokenInspectorPolicyForm token={token} campaign={campaign} effectReveals={[]} />);
    fireEvent.click(screen.getByRole('checkbox', { name: name('effects') }));
    expect(state.mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'update_campaign_meta',
        name: null,
        description: null,
        token_inspector: {
          reveal: ['name', 'size', 'character', 'conditions', 'effects'],
          conceal: [],
        },
      },
      expectedVersion: 7,
    });
  });

  it('closes a campaign field without leaving it in the reveal list', () => {
    render(<TokenInspectorPolicyForm token={token} campaign={campaign} effectReveals={[]} />);
    fireEvent.click(screen.getByRole('checkbox', { name: name('character') }));
    expect(state.mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'update_campaign_meta',
        name: null,
        description: null,
        token_inspector: {
          reveal: ['name', 'size', 'conditions'],
          conceal: [],
        },
      },
      expectedVersion: 7,
    });
  });

  it('closes a single row for this token with the token version', () => {
    render(<TokenInspectorPolicyForm token={token} campaign={campaign} effectReveals={[]} />);
    fireEvent.change(screen.getByRole('combobox', { name: name('name') }), {
      target: { value: 'close' },
    });
    expect(state.mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'update_token',
        id: 't1',
        name: null,
        rotation: null,
        scale: null,
        layer: null,
        visible: null,
        disposition: null,
        inspector: { reveal: [], conceal: ['name'] },
      },
      expectedVersion: 4,
    });
  });

  it('takes a row back from the override', () => {
    const overridden: TokenDto = {
      ...token,
      inspector: { reveal: ['position'], conceal: ['name'] },
    };
    render(<TokenInspectorPolicyForm token={overridden} campaign={campaign} effectReveals={[]} />);
    fireEvent.change(screen.getByRole('combobox', { name: name('position') }), {
      target: { value: 'inherit' },
    });
    expect(state.mutate).toHaveBeenLastCalledWith({
      intent: {
        kind: 'update_token',
        id: 't1',
        name: null,
        rotation: null,
        scale: null,
        layer: null,
        visible: null,
        disposition: null,
        inspector: { reveal: [], conceal: ['name'] },
      },
      expectedVersion: 4,
    });
  });

  it('sends an empty override, which the core turns into "follow the campaign"', () => {
    const overridden: TokenDto = { ...token, inspector: { reveal: [], conceal: ['name'] } };
    render(<TokenInspectorPolicyForm token={overridden} campaign={campaign} effectReveals={[]} />);
    fireEvent.change(screen.getByRole('combobox', { name: name('name') }), {
      target: { value: 'inherit' },
    });
    expect(state.mutate).toHaveBeenLastCalledWith({
      intent: {
        kind: 'update_token',
        id: 't1',
        name: null,
        rotation: null,
        scale: null,
        layer: null,
        visible: null,
        disposition: null,
        inspector: { reveal: [], conceal: [] },
      },
      expectedVersion: 4,
    });
  });

  it('says which rows an active effect opens', () => {
    render(
      <TokenInspectorPolicyForm
        token={token}
        campaign={campaign}
        effectReveals={['position', 'effects']}
      />,
    );
    expect(screen.getByText(/inspector.policy.effectHint/)).toBeInTheDocument();
  });
});
