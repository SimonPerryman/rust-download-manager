import { useState } from "react";
import { CopyDiagnosticsButton, OpenLogsFolderButton } from "./TroubleshootingButtons";

// The settings menu's last group: kept quiet (a muted heading, small ghost
// buttons) because it's for the rare bug report, yet always there.
export function TroubleshootingSection() {
  const [problem, setProblem] = useState<string | null>(null);
  return (
    <section className="ts" aria-labelledby="troubleshooting-heading">
      <h3 id="troubleshooting-heading">Troubleshooting</h3>
      <div className="ts-row">
        <OpenLogsFolderButton size="sm" onError={setProblem} />
        <CopyDiagnosticsButton size="sm" onError={setProblem} />
      </div>
      {problem ? (
        <p className="ts-error" role="alert">
          {problem}
        </p>
      ) : (
        <p className="ts-hint">Diagnostics hide links, file names and folders.</p>
      )}
    </section>
  );
}
