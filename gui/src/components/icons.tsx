// Inline lucide-style SVG icons ported verbatim from the mockup. Keeping them
// as small components avoids an icon-library dependency.

type Props = { size?: number; className?: string; strokeWidth?: number };

function svg(children: React.ReactNode, { size = 16, className, strokeWidth = 2 }: Props) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      {children}
    </svg>
  );
}

export const DownloadMark = (p: Props) =>
  svg(
    <>
      <path d="M12 3v12" />
      <path d="m7 11 5 5 5-5" />
      <path d="M5 21h14" />
    </>,
    { strokeWidth: 2.5, ...p },
  );

export const Pause = (p: Props) =>
  svg(
    <>
      <rect x="6" y="4" width="4" height="16" rx="1" />
      <rect x="14" y="4" width="4" height="16" rx="1" />
    </>,
    p,
  );

export const Play = (p: Props) => svg(<polygon points="6 3 20 12 6 21 6 3" />, p);

export const Plus = (p: Props) =>
  svg(
    <>
      <path d="M5 12h14" />
      <path d="M12 5v14" />
    </>,
    { strokeWidth: 2.25, ...p },
  );

export const X = (p: Props) =>
  svg(
    <>
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </>,
    p,
  );

export const Retry = (p: Props) =>
  svg(
    <>
      <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
      <path d="M3 3v5h5" />
    </>,
    p,
  );

export const Trash = (p: Props) =>
  svg(
    <>
      <path d="M3 6h18" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
    </>,
    p,
  );

export const FileIcon = (p: Props) =>
  svg(
    <>
      <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
      <path d="M14 2v6h6" />
    </>,
    { strokeWidth: 1.75, ...p },
  );

export const Clock = (p: Props) =>
  svg(
    <>
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </>,
    { strokeWidth: 2.25, ...p },
  );

export const Check = (p: Props) =>
  svg(<polyline points="20 6 9 17 4 12" />, { strokeWidth: 2.5, ...p });

export const AlertTriangle = (p: Props) =>
  svg(
    <>
      <path d="M12 9v4" />
      <path d="M12 17h.01" />
      <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
    </>,
    p,
  );

export const ShieldCheck = (p: Props) =>
  svg(
    <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" />,
    p,
  );

export const ExternalLink = (p: Props) =>
  svg(
    <>
      <path d="M15 3h6v6" />
      <path d="M10 14 21 3" />
      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
    </>,
    p,
  );

export const Folder = (p: Props) =>
  svg(
    <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />,
    p,
  );

export const Sun = (p: Props) =>
  svg(
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2" />
      <path d="M12 20v2" />
      <path d="m4.93 4.93 1.41 1.41" />
      <path d="m17.66 17.66 1.41 1.41" />
      <path d="M2 12h2" />
      <path d="M20 12h2" />
      <path d="m6.34 17.66-1.41 1.41" />
      <path d="m19.07 4.93-1.41 1.41" />
    </>,
    p,
  );

export const Moon = (p: Props) => svg(<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />, p);

export const Monitor = (p: Props) =>
  svg(
    <>
      <rect x="2" y="3" width="20" height="14" rx="2" />
      <path d="M8 21h8" />
      <path d="M12 17v4" />
    </>,
    p,
  );

export const Sliders = (p: Props) =>
  svg(
    <>
      <path d="M20 7h-9" />
      <path d="M14 17H5" />
      <circle cx="17" cy="17" r="3" />
      <circle cx="7" cy="7" r="3" />
    </>,
    p,
  );
