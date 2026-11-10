// End-to-end: a command that panics (a bug) rejects its `invoke` with an
// internal error instead of killing the app (a synchronous command) or never
// answering (an async one), and the app keeps answering afterwards. Uses the
// fault-injection build's test-only commands.

import { browser, expect } from "@wdio/globals";

/** Invoke `command` straight over IPC; settles as `resolved …`/`rejected …`. */
async function outcome(command: string): Promise<string> {
  return browser.execute(
    (cmd: string) =>
      new Promise<string>((settle) => {
        type Invoke = (c: string) => Promise<unknown>;
        const { invoke } = (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } })
          .__TAURI_INTERNALS__;
        const timer = setTimeout(() => settle("no answer"), 10_000);
        invoke(cmd).then(
          (value) => {
            clearTimeout(timer);
            settle(`resolved ${JSON.stringify(value)}`);
          },
          (error) => {
            clearTimeout(timer);
            settle(`rejected ${String(error)}`);
          },
        );
      }),
    command,
  );
}

describe("rdm GUI command panics", () => {
  for (const command of ["test_panic", "test_panic_async"]) {
    it(`rejects ${command} with an internal error and keeps working`, async () => {
      expect(await outcome(command)).toBe(
        `rejected rdm hit an internal error running ${command}, a bug. Details are in the log.`,
      );
      expect(await outcome("list")).toMatch(/^resolved \[/);
    });
  }
});
