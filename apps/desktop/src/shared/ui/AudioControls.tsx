import { type ButtonHTMLAttributes } from 'react';
import { Icon, type IconName } from './Icon';
import './audioPlayers.css';

export function audioTime(seconds: number) {
  const safe = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  return `${Math.floor(safe / 60)}:${String(safe % 60).padStart(2, '0')}`;
}

export function AudioButton({
  label,
  icon,
  paused,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; icon?: IconName; paused?: boolean }) {
  return (
    <button type="button" aria-label={label} title={label} {...props}>
      {paused === false ? (
        <svg
          className="icon"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="currentColor"
          aria-hidden="true"
        >
          <rect x="6" y="4" width="4" height="16" rx="1" />
          <rect x="14" y="4" width="4" height="16" rx="1" />
        </svg>
      ) : (
        <Icon name={icon ?? 'play'} size={18} />
      )}
    </button>
  );
}
