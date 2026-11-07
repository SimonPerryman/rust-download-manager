import { svg, type Props } from "./svg";

export const CheckCircle = (p: Props) =>
  svg(
    <>
      <circle cx="12" cy="12" r="8.25" />
      <path d="M8.25 12.25l2.5 2.5 5-5" />
    </>,
    p,
  );
