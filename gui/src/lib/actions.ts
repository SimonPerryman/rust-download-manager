// What the "When downloads finish" rows say: pure text from the engine's settings and status.

import { wording } from "./startAt";
import type { CommandAction, PowerAction, PowerStatus, WaitReason } from "./types";

export const POWER_ACTIONS: readonly PowerAction[] = ["shutdown", "sleep", "hibernate"];

/** The action as a menu choice. */
export const powerLabel = (a: PowerAction): string =>
  a === "shutdown" ? "Shut down" : a === "sleep" ? "Sleep" : "Hibernate";

/** What a power action does, as the tooltip on its choice; null when the name says it. */
export const powerHint = (a: PowerAction): string | null =>
  a === "sleep"
    ? "Low power; wakes in seconds"
    : a === "hibernate"
      ? "Saves to disk and powers off completely"
      : null;

/** The power row's title; the chosen action shows on its chip, not here. */
export const POWER_TITLE = "After all downloads";

/** The command row's title. */
export const COMMAND_TITLE = "After each download";

/** The command as typed in a shell would read it, for the row: `scan.exe {path}`. */
export function commandLine(c: CommandAction): string {
  return [c.program, ...c.args].filter((p) => p !== "").join(" ");
}

/** The Arguments box holds one argument per line, so none needs quoting. */
export const argsText = (args: readonly string[]): string => args.join("\n");

/** The Arguments box back into the list: blank lines dropped, nothing else touched. */
export const argsFromText = (text: string): string[] =>
  text.split(/\r?\n/).filter((line) => line.trim() !== "");

/** Why the command can't be saved, or null. */
export const commandProblem = (program: string): string | null =>
  program.trim() === "" ? "Enter the program to run" : null;

/** What a switched-on power action is waiting for, as the row says it. */
export function waitText(reason: WaitReason, now?: Date): string {
  switch (reason.kind) {
    case "running":
      return "Waiting for downloads to finish";
    case "queued":
      return "Waiting for queued files";
    case "window":
      return "Waiting for the queue window to open";
    case "scheduled":
      return `Waiting for a start ${wording(reason.at, now ?? new Date())}`;
    case "command":
      return "Waiting for the command to finish";
  }
}

/** The power row's second line: what it waits for, or why it failed; null when there is nothing to say. */
export function powerNote(status: PowerStatus, action: PowerAction, now?: Date): string | null {
  switch (status.state) {
    case "waiting":
      return waitText(status.reason, now);
    case "failed":
      return `Couldn't ${powerLabel(action).toLowerCase()}: ${status.message}`;
    default:
      return null;
  }
}

/** The countdown notice: "Shutting down in 42 s". */
export function countdownText(action: PowerAction, secondsLeft: number): string {
  const verb =
    action === "shutdown" ? "Shutting down" : action === "sleep" ? "Going to sleep" : "Hibernating";
  return `${verb} in ${secondsLeft} s`;
}
