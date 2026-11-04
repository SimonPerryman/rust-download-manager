import { svg, type Props } from "./svg";

export const X = (p: Props) =>
  svg(
    <>
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </>,
    p,
  );
