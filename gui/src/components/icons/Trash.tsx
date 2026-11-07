// From Lucide (ISC); see THIRD-PARTY-NOTICES.md.
import { svg, type Props } from "./svg";

export const Trash = (p: Props) =>
  svg(
    <>
      <path d="M3 6h18" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
    </>,
    p,
  );
