import { AlertTriangle } from "../icons/AlertTriangle";

// A finished row whose after-download command failed: the download is fine, so this is an
// amber icon beside the status text, with the reason as its tooltip and accessible label.
export function CommandWarning({ message }: { message: string }) {
  return (
    <span
      className="row-warning"
      data-testid="command-warning"
      role="img"
      aria-label={`Command failed: ${message}`}
      title={`Command failed: ${message}`}
    >
      <AlertTriangle size={14} strokeWidth={2} />
    </span>
  );
}
