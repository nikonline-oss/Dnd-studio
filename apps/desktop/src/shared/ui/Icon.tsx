// Inline SVG icons (24×24 grid, stroked with `currentColor`). Drawn here rather than taken from
// an icon package: a dozen glyphs do not justify a dependency (CLAUDE.md §14).

const PATHS = {
  plus: 'M12 5v14M5 12h14',
  image: 'M4 5h16v14H4zM4 16l5-5 4 4 2-2 5 5M15.5 9.5h.01',
  trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6',
  eye: 'M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
  eyeOff:
    'M3 3l18 18M10.6 5.1A10 10 0 0 1 12 5c6.5 0 10 7 10 7a17 17 0 0 1-3.2 4.2M6.6 6.6C3.9 8.4 2 12 2 12s3.5 7 10 7a9.8 9.8 0 0 0 5.4-1.6M9.9 9.9a3 3 0 0 0 4.2 4.2',
  fit: 'M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5',
  send: 'M4 12l16-8-6 16-2.5-6.5L4 12z',
  dice: 'M12 2l9 5v10l-9 5-9-5V7l9-5zM12 2v20M3 7l9 5 9-5M7.5 14.5h.01M16.5 14.5h.01M12 6.5h.01',
  map: 'M3 6l6-3 6 3 6-3v15l-6 3-6-3-6 3V6zM9 3v15M15 6v15',
  users:
    'M16 20v-1.5a3.5 3.5 0 0 0-3.5-3.5h-5A3.5 3.5 0 0 0 4 18.5V20M10 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM20 20v-1.5a3.5 3.5 0 0 0-2.5-3.35M15.5 4.15a3.5 3.5 0 0 1 0 6.7',
  book: 'M5 4.5A1.5 1.5 0 0 1 6.5 3H19v15H6.5A1.5 1.5 0 0 0 5 19.5zM5 19.5A1.5 1.5 0 0 0 6.5 21H19v-3',
  folder:
    'M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v8.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z',
  check: 'M5 12.5l4.5 4.5L19 7',
  x: 'M6 6l12 12M18 6L6 18',
  grid: 'M4 4h16v16H4zM4 9.33h16M4 14.67h16M9.33 4v16M14.67 4v16',
  lock: 'M6 11h12v9H6zM8.5 11V8a3.5 3.5 0 0 1 7 0v3',
  user: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM5 20v-1a5 5 0 0 1 5-5h4a5 5 0 0 1 5 5v1',
  sparkle: 'M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z',
  hand: 'M8 13V5.5a1.5 1.5 0 0 1 3 0V12M11 11V4.5a1.5 1.5 0 0 1 3 0V12M14 11.5V6a1.5 1.5 0 0 1 3 0v7c0 4-2.5 7-6 7h-1c-2.5 0-4-1.2-5.3-3l-2.4-3.5a1.5 1.5 0 0 1 2.4-1.8L8 14',
  square: 'M5 5h14v14H5z',
  brush:
    'M14.5 4.5l5 5-7.5 7.5-5-5zM7 12l-2.5 2.5c-1.2 1.2-.8 3-2 4.5 2.5.8 5 .3 6.5-1.2L11.5 15.5',
  cloud: 'M7 18h10a4 4 0 0 0 .6-7.95A6 6 0 0 0 6.1 9.6 4.2 4.2 0 0 0 7 18z',
  folderOpen:
    'M3 7a1.5 1.5 0 0 1 1.5-1.5H9l2 2h7.5A1.5 1.5 0 0 1 20 9v1.5M3 7v10.5A1.5 1.5 0 0 0 4.5 19H17l4-8.5H7.5L3 19',
  chevron: 'M9.5 6l6 6-6 6',
  puzzle:
    'M10 4a2 2 0 1 1 4 0v1h4v4h-1a2 2 0 1 0 0 4h1v4h-4v-1a2 2 0 1 0-4 0v1H6v-4h1a2 2 0 1 0 0-4H6V5h4z',
  folderPlus:
    'M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2.5h8.5A1.5 1.5 0 0 1 21 9v8.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5zM12 10.5v6M9 13.5h6',
  filePlus: 'M6 3h8l4 4v14H6zM14 3v4h4M12 11v6M9 14h6',
  collapse: 'M5 5h14v14H5zM9 12h6',
  board: 'M3 4h7v5H3zM14 4h7v5h-7zM8.5 15h7v5h-7zM6.5 9l4 6M17.5 9l-4 6',
  link: 'M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1',
  note: 'M5 4h14v12l-4 4H5zM15 20v-4h4M8 9h8M8 13h5',
  zone: 'M4 4h3M10 4h4M17 4h3v3M20 10v4M20 17v3h-3M14 20h-4M7 20H4v-3M4 14v-4M4 7V4',
  pointer: 'M6 3l12 9-5.5 1 3 6.5-2.5 1-3-6.5L6 18z',
  grip: 'M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01',
  bolt: 'M13 2L4 14h7l-1 8 9-12h-7z',
  table: 'M4 5h16v14H4zM4 10h16M4 15h16M10 5v14',
  download: 'M12 4v11M7 10l5 5 5-5M5 20h14',
  upload: 'M12 20V9M7 14l5-5 5 5M5 4h14',
  music: 'M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM20 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z',
  volume: 'M4 9h4l5-4v14l-5-4H4zM16 9a4 4 0 0 1 0 6M18.5 6.5a8 8 0 0 1 0 11',
  volumeOff: 'M4 9h4l5-4v14l-5-4H4zM16 9l5 6M21 9l-5 6',
  play: 'M7 4l13 8-13 8z',
  stop: 'M6 6h12v12H6z',
} as const;

export type IconName = keyof typeof PATHS;

interface IconProps {
  name: IconName;
  size?: number;
  className?: string;
}

/** A decorative icon: hidden from assistive technology; label the control that holds it. */
export function Icon({ name, size = 16, className }: IconProps) {
  return (
    <svg
      className={className ? `icon ${className}` : 'icon'}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
