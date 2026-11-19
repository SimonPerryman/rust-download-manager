import { describe, expect, it } from "vitest";
import {
  argsFromText,
  argsText,
  commandLine,
  commandParts,
  insertAt,
  commandProblem,
  countdownText,
  powerNote,
  powerHint,
  POWER_TITLE,
  COMMAND_TITLE,
  waitText,
} from "./actions";

describe("the command row", () => {
  it("reads as the program and its arguments", () => {
    expect(commandLine({ enabled: true, program: "scan.exe", args: ["{download_path}"] })).toBe(
      "scan.exe {download_path}",
    );
    expect(commandLine({ enabled: false, program: "", args: [] })).toBe("");
  });

  it("keeps one argument per line, spaces and all, and drops blank lines", () => {
    expect(argsFromText("--name\n{download_name} x;y\n\n  \n{download_path}\r\n")).toEqual([
      "--name",
      "{download_name} x;y",
      "{download_path}",
    ]);
    expect(argsFromText(argsText(["a b", "c"]))).toEqual(["a b", "c"]);
  });

  it("needs a program", () => {
    expect(commandProblem("  ")).not.toBeNull();
    expect(commandProblem("scan.exe")).toBeNull();
  });
});

describe("the command variables", () => {
  it("splits a command line into plain text and variable tokens", () => {
    expect(commandParts("scan.exe --in {download_path} [{download_name}]")).toEqual([
      { text: "scan.exe --in " },
      { text: "{download_path}", hint: expect.stringContaining("with its folder") },
      { text: " [" },
      { text: "{download_name}", hint: expect.stringContaining("file's name") },
      { text: "]" },
    ]);
  });

  it("leaves old names, unknown braces and empty lines alone", () => {
    expect(commandParts("a {path} {name} {x {")).toEqual([{ text: "a {path} {name} {x {" }]);
    expect(commandParts("")).toEqual([]);
    expect(commandParts("{download_path}{download_path}")).toHaveLength(2);
  });
});

describe("inserting at the caret", () => {
  const v = "{download_name}";
  it("inserts at the start, middle and end", () => {
    expect(insertAt("ab", 0, 0, v)).toEqual({ text: `${v}ab`, caret: v.length });
    expect(insertAt("ab", 1, 1, v)).toEqual({ text: `a${v}b`, caret: 1 + v.length });
    expect(insertAt("ab", 2, 2, v)).toEqual({ text: `ab${v}`, caret: 2 + v.length });
  });

  it("replaces a selection", () => {
    expect(insertAt("abcd", 1, 3, v)).toEqual({ text: `a${v}d`, caret: 1 + v.length });
    expect(insertAt("abcd", 3, 1, v).text).toBe(`a${v}d`);
  });
});

describe("the power row", () => {
  it("keeps its title whatever the action", () => {
    expect(POWER_TITLE).toBe("After all downloads");
    expect(COMMAND_TITLE).toBe("After each download");
  });

  it("explains sleep and hibernate, and leaves shut down plain", () => {
    expect(powerHint("sleep")).toBe("Low power; wakes in seconds");
    expect(powerHint("hibernate")).toBe("Saves to disk and powers off completely");
    expect(powerHint("shutdown")).toBeNull();
  });

  it("says what it is waiting for", () => {
    const now = new Date(2026, 2, 9, 12, 0);
    expect(waitText({ kind: "running" })).toBe("Waiting for downloads to finish");
    expect(waitText({ kind: "queued" })).toBe("Waiting for queued files");
    expect(waitText({ kind: "window" })).toBe("Waiting for the queue window to open");
    expect(waitText({ kind: "command" })).toBe("Waiting for the command to finish");
    expect(waitText({ kind: "scheduled", at: "2026-03-09T23:00" }, now)).toBe(
      "Waiting for a start today 23:00",
    );
    expect(waitText({ kind: "scheduled", at: "2026-03-12T03:00" }, now)).toBe(
      "Waiting for a start Thu 03:00",
    );
  });

  it("has a note only while waiting or after a failure", () => {
    expect(powerNote({ state: "off" }, "sleep")).toBeNull();
    expect(powerNote({ state: "ready" }, "sleep")).toBeNull();
    expect(powerNote({ state: "waiting", reason: { kind: "running" } }, "sleep")).toBe(
      "Waiting for downloads to finish",
    );
    expect(powerNote({ state: "failed", message: "denied" }, "hibernate")).toBe(
      "Couldn't hibernate: denied",
    );
  });

  it("counts down in the action's own words", () => {
    expect(countdownText("shutdown", 42)).toBe("Shutting down in 42 s");
    expect(countdownText("sleep", 5)).toBe("Going to sleep in 5 s");
    expect(countdownText("hibernate", 60)).toBe("Hibernating in 60 s");
  });
});
