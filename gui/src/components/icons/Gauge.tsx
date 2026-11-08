// From Lucide (ISC); see THIRD-PARTY-NOTICES.md.
import { svg, type Props } from "./svg";

export const Gauge = (p: Props) =>
  svg(
    <>
      <path d="m12 14 4-4" />
      <path d="M3.34 19a10 10 0 1 1 17.32 0" />
    </>,
    p,
  );
