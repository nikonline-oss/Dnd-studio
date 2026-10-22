import { useEffect, useState, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Icon, type IconName } from './Icon';

interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  icon: IconName;
  /** Accessible name and tooltip. */
  label: string;
  /** Text next to the icon; without it the button is icon-only. */
  children?: ReactNode;
  variant?: 'ghost' | 'primary' | 'default';
}

export function IconButton({
  icon,
  label,
  children,
  variant = 'ghost',
  className,
  ...rest
}: IconButtonProps) {
  const classes = ['icon-button', `btn-${variant}`, children ? 'with-text' : '', className ?? '']
    .filter(Boolean)
    .join(' ');
  return (
    <button type="button" aria-label={label} title={label} className={classes} {...rest}>
      <Icon name={icon} />
      {children}
    </button>
  );
}

/** How long a delete waits for the confirming click. */
export const CONFIRM_MS = 4000;

interface ConfirmDeleteProps {
  /** What is deleted, for the labels ("Delete map Crypt"). */
  label: string;
  onConfirm: () => void;
  disabled?: boolean;
}

/**
 * A delete that needs a second click: the first one turns the button into "Delete?" for a few
 * seconds. Prevents losing a map or entry to a stray click next to its row.
 */
export function ConfirmDelete({ label, onConfirm, disabled }: ConfirmDeleteProps) {
  const { t } = useTranslation();
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => {
      setArmed(false);
    }, CONFIRM_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [armed]);

  if (!armed) {
    return (
      <IconButton
        icon="trash"
        label={label}
        className="danger-hover"
        disabled={disabled}
        onClick={(e) => {
          e.stopPropagation();
          setArmed(true);
        }}
      />
    );
  }
  return (
    <button
      type="button"
      className="confirm-delete"
      disabled={disabled}
      onBlur={() => {
        setArmed(false);
      }}
      autoFocus
      onClick={(e) => {
        e.stopPropagation();
        setArmed(false);
        onConfirm();
      }}
    >
      <Icon name="trash" />
      {t('common.confirmDelete')}
    </button>
  );
}
