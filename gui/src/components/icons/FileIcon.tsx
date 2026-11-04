import { svg, type Props } from "./svg";

export const FileIcon = (p: Props) =>
  svg(
    <>
      <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" />
      <path d="M14 2v6h6" />
    </>,
    { strokeWidth: 1.75, ...p },
  );
