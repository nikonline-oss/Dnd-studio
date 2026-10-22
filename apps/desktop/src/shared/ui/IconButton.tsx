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

/** How long a destructive button waits for the confirming click. */
export const CONFIRM_MS = 4000;

interface ConfirmButtonProps {
  icon: IconName;
  /** Accessible name and tooltip of the unarmed button. */
  label: string;
  /** Text of the armed button ("Delete?"). */
  confirmLabel: string;
  onConfirm: () => void;
  disabled?: boolean;
}

/**
 * A destructive action that needs a second click: the first one turns the button into
 * `confirmLabel` for a few seconds. Prevents losing work to a stray click.
 */
export function ConfirmButton({
  icon,
  label,
  confirmLabel,
  onConfirm,
  disabled,
}: ConfirmButtonProps) {
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
        icon={icon}
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
      <Icon name={icon} />
      {confirmLabel}
    </button>
  );
}

/** Delete with confirmation; `label` names what is deleted ("Delete map"). */
export function ConfirmDelete({
  label,
  onConfirm,
  disabled,
}: Pick<ConfirmButtonProps, 'label' | 'onConfirm' | 'disabled'>) {
  const { t } = useTranslation();
  return (
    <ConfirmButton
      icon="trash"
      label={label}
      confirmLabel={t('common.confirmDelete')}
      onConfirm={onConfirm}
      {...(disabled === undefined ? {} : { disabled })}
    />
  );
}
