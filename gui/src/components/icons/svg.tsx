// Inline SVG icons (Console stroke weight); many are copied from Lucide (ISC),
// see THIRD-PARTY-NOTICES.md. Keeping them as small components avoids an
// icon-library dependency. Decorative: buttons carry the name.

export type Props = { size?: number; className?: string; strokeWidth?: number };

export function svg(
  children: React.ReactNode,
  { size = 16, className, strokeWidth = 1.75 }: Props,
) {
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
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}
