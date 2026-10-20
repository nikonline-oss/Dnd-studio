import { Component, type ErrorInfo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

interface Props {
  /** Shown in the fallback and the log, e.g. the feature name. */
  name: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Isolates a feature (CLAUDE.md §10): a crash in one panel shows a fallback with "Try again"
 * instead of taking down the whole window.
 */
export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`[${this.props.name}] crashed`, error, info.componentStack);
  }

  override render() {
    if (this.state.error) {
      return (
        <Fallback
          name={this.props.name}
          onRetry={() => {
            this.setState({ error: null });
          }}
        />
      );
    }
    return this.props.children;
  }
}

function Fallback({ name, onRetry }: { name: string; onRetry: () => void }) {
  const { t } = useTranslation();
  return (
    <div role="alert" className="error-fallback">
      <p>{t('errorBoundary.message', { name })}</p>
      <button type="button" onClick={onRetry}>
        {t('errorBoundary.retry')}
      </button>
    </div>
  );
}
