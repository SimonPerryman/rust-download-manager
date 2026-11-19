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

/** The variables a command's arguments can use, filled in per finished download. */
export const COMMAND_VARIABLES = [
  {
    token: "{download_path}",
    label: "Download path",
    hint: "The finished file, with its folder. Example: C:\\Downloads\\report.zip",
  },
  {
    token: "{download_name}",
    label: "Download name",
    hint: "Just the file's name. Example: report.zip",
  },
] as const;

/** A piece of a command line: plain text, or a variable (with its explanation). */
export type CommandPart = { text: string; hint?: string };

/** Splits a command line so each known variable is its own part; anything else stays as typed. */
export function commandParts(line: string): CommandPart[] {
  const parts: CommandPart[] = [];
  let plain = "";
  let rest = line;
  while (rest !== "") {
    const v = COMMAND_VARIABLES.find((c) => rest.startsWith(c.token));
    if (v) {
      if (plain) parts.push({ text: plain });
      plain = "";
      parts.push({ text: v.token, hint: v.hint });
      rest = rest.slice(v.token.length);
    } else {
      plain += rest[0];
      rest = rest.slice(1);
    }
  }
  if (plain) parts.push({ text: plain });
  return parts;
}

/** `text` with `insert` replacing the selection `start..end`, and where the caret goes after it. */
export function insertAt(
  text: string,
  start: number,
  end: number,
  insert: string,
): { text: string; caret: number } {
  const lo = Math.min(start, end);
  const hi = Math.max(start, end);
  return { text: text.slice(0, lo) + insert + text.slice(hi), caret: lo + insert.length };
}

/** The command as typed in a shell would read it, for the row: `scan.exe {download_path}`. */
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
