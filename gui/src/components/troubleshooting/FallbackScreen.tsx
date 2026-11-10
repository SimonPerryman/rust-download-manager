import { useEffect, useRef, useState } from "react";
import * as api from "../../lib/api";
import { AlertTriangle } from "../icons/AlertTriangle";
import { Retry } from "../icons/Retry";
import { CopyDiagnosticsButton, OpenLogsFolderButton } from "./TroubleshootingButtons";

/**
 * What the window shows once drawing it failed: a plain sentence, Reload and
 * the troubleshooting actions, and the technical error behind Show details.
 * `details` (the error and its stacks) also goes into Copy diagnostics.
 */
export function FallbackScreen({ message, details }: { message: string; details: string }) {
  const [problem, setProblem] = useState<string | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  // It replaces the whole window: take focus, so keyboard and screen reader
  // users land on what happened.
  useEffect(() => heading.current?.focus(), []);
  return (
    <main className="fallback" data-testid="fallback">
      <div className="fb">
        <div className="fb-mark">
          <AlertTriangle />
        </div>
        <h1 ref={heading} tabIndex={-1} className="outline-none">
          Something went wrong
        </h1>
        <p className="fb-say">
          rdm hit a problem drawing this window. Your downloads keep running.
        </p>
        <div className="fb-acts">
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => window.location.reload()}
          >
            <Retry />
            Reload
          </button>
          <OpenLogsFolderButton size="md" onError={setProblem} />
          <CopyDiagnosticsButton size="md" extra={details} onError={setProblem} />
        </div>
        {problem && (
          <p className="fb-report text-danger-ink" role="alert">
            {problem}
          </p>
        )}
        <p className="fb-report">
          To report it, copy diagnostics and paste them into a{" "}
          <button
            type="button"
            className="btn-link"
            onClick={() => {
              setProblem(null);
              api.openBugReport().catch((e) => setProblem(`Couldn't open GitHub: ${e}`));
            }}
          >
            new issue on GitHub
          </button>
          .
        </p>
        <details className="fb-details">
          <summary>Show details</summary>
          <pre data-testid="fallback-error">{message}</pre>
        </details>
      </div>
    </main>
  );
}
