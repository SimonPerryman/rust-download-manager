import { svg, type Props } from "./svg";

export const DownloadMark = (p: Props) =>
  svg(
    <>
      <path d="M12 3v12" />
      <path d="m7 11 5 5 5-5" />
      <path d="M5 21h14" />
    </>,
    { strokeWidth: 2.5, ...p },
  );
