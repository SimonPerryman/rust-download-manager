import { svg, type Props } from "./svg";

export const DownloadAgain = (p: Props) =>
  svg(
    <>
      <path d="M12 4v9M8 9.5l4 4 4-4" />
      <path d="M5 16v2.5a1.5 1.5 0 0 0 1.5 1.5h11a1.5 1.5 0 0 0 1.5-1.5V16" />
    </>,
    p,
  );
