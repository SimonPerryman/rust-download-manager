import { svg, type Props } from "./svg";

export const Clock = (p: Props) =>
  svg(
    <>
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </>,
    { strokeWidth: 2.25, ...p },
  );
