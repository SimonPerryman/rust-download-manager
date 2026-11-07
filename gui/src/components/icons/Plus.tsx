// From Lucide (ISC); see THIRD-PARTY-NOTICES.md.
import { svg, type Props } from "./svg";

export const Plus = (p: Props) =>
  svg(
    <>
      <path d="M5 12h14" />
      <path d="M12 5v14" />
    </>,
    { strokeWidth: 2.25, ...p },
  );
