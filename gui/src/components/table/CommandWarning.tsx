import { AlertTriangle } from "../icons/AlertTriangle";

// A finished row whose after-download command failed: the download is fine, so this is a
// warning beside its name, with the reason as its tooltip.
export function CommandWarning({ message }: { message: string }) {
  return (
    <span className="row-warning" data-testid="command-warning" title={message}>
      <AlertTriangle size={14} strokeWidth={2} />
      Command failed
    </span>
  );
}
