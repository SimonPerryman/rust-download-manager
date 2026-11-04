import { svg, type Props } from "./svg";

export const Check = (p: Props) =>
  svg(<polyline points="20 6 9 17 4 12" />, { strokeWidth: 2.5, ...p });
