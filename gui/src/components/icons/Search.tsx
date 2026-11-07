import { svg, type Props } from "./svg";

export const Search = (p: Props) =>
  svg(
    <>
      <circle cx="11" cy="11" r="6" />
      <path d="M19.5 19.5l-4.25-4.25" />
    </>,
    p,
  );
