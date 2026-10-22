import { useId, type ReactNode } from 'react';
import { Icon, type IconName } from './Icon';

interface NavSectionProps {
  icon: IconName;
  title: string;
  count?: number;
  /** Compact buttons on the right of the header. */
  actions?: ReactNode;
  children: ReactNode;
}

/** A navigator section (IDE style): small caps header with actions, then a list of rows. */
export function NavSection({ icon, title, count, actions, children }: NavSectionProps) {
  const titleId = useId();
  return (
    <section className="nav-section list-panel" aria-labelledby={titleId}>
      <header className="nav-header">
        <Icon name={icon} className="nav-icon" />
        <h2 id={titleId}>{title}</h2>
        {count !== undefined && count > 0 && <span className="nav-count">{count}</span>}
        <span className="nav-actions">{actions}</span>
      </header>
      {children}
    </section>
  );
}
