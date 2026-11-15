// From Lucide (ISC); see THIRD-PARTY-NOTICES.md.
import { svg, type Props } from "./svg";

export const Info = (p: Props) =>
  svg(
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 16v-4" />
      <path d="M12 8h.01" />
    </>,
    p,
  );
