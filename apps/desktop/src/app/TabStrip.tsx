import { useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Icon } from '@/shared/ui/Icon';

/** Keep scrolling local to the tab bar: neither selection nor the document moves. */
export function TabStrip({ children, activeKey }: { children: ReactNode; activeKey: string }) {
  const { t } = useTranslation();
  const viewport = useRef<HTMLDivElement>(null);
  const track = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ overflow: false, left: false, right: false });

  useEffect(() => {
    const strip = viewport.current;
    const content = track.current;
    if (!strip || !content) return;
    const update = () => {
      const max = strip.scrollWidth - strip.clientWidth;
      const next = {
        overflow: max > 1,
        left: strip.scrollLeft > 1,
        right: strip.scrollLeft < max - 1,
      };
      setEdges((previous) =>
        previous.overflow === next.overflow &&
        previous.left === next.left &&
        previous.right === next.right
          ? previous
          : next,
      );
    };
    const reveal = () => {
      const selected = content.querySelector<HTMLElement>('[aria-selected="true"]');
      if (selected) {
        const bounds = strip.getBoundingClientRect();
        const tab = selected.getBoundingClientRect();
        if (tab.left < bounds.left) strip.scrollLeft += tab.left - bounds.left;
        else if (tab.right > bounds.right) strip.scrollLeft += tab.right - bounds.right;
      }
      update();
    };
    const wheel = (event: WheelEvent) => {
      if (event.ctrlKey || strip.scrollWidth <= strip.clientWidth) return;
      const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? strip.clientWidth : 1;
      strip.scrollLeft = Math.max(
        0,
        Math.min(strip.scrollWidth - strip.clientWidth, strip.scrollLeft + delta * unit),
      );
      event.preventDefault();
      update();
    };
    const observer = new ResizeObserver(reveal);
    observer.observe(strip);
    observer.observe(content);
    const frame = requestAnimationFrame(reveal);
    strip.addEventListener('scroll', update);
    strip.addEventListener('wheel', wheel, { passive: false });
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      strip.removeEventListener('scroll', update);
      strip.removeEventListener('wheel', wheel);
    };
  }, [activeKey, children]);

  const scroll = (direction: number) => {
    const strip = viewport.current;
    strip?.scrollBy({
      left: direction * Math.max(120, strip.clientWidth * 0.7),
      behavior: 'smooth',
    });
  };

  return (
    <div className="center-tabs">
      {edges.overflow && (
        <button
          type="button"
          className="tab-scroll tab-scroll-left"
          aria-label={t('tabs.scrollLeft')}
          title={t('tabs.scrollLeft')}
          disabled={!edges.left}
          onClick={() => {
            scroll(-1);
          }}
        >
          <Icon name="chevron" size={16} />
        </button>
      )}
      <div role="tablist" className="tabs" ref={viewport}>
        <div className="tab-track" ref={track}>
          {children}
        </div>
      </div>
      {edges.overflow && (
        <button
          type="button"
          className="tab-scroll"
          aria-label={t('tabs.scrollRight')}
          title={t('tabs.scrollRight')}
          disabled={!edges.right}
          onClick={() => {
            scroll(1);
          }}
        >
          <Icon name="chevron" size={16} />
        </button>
      )}
    </div>
  );
}
