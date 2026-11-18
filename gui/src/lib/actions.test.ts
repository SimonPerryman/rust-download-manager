import { describe, expect, it } from "vitest";
import {
  argsFromText,
  argsText,
  commandLine,
  commandProblem,
  countdownText,
  powerNote,
  powerTitle,
  waitText,
} from "./actions";

describe("the command row", () => {
  it("reads as the program and its arguments", () => {
    expect(commandLine({ enabled: true, program: "scan.exe", args: ["{path}"] })).toBe(
      "scan.exe {path}",
    );
    expect(commandLine({ enabled: false, program: "", args: [] })).toBe("");
  });

  it("keeps one argument per line, spaces and all, and drops blank lines", () => {
    expect(argsFromText("--name\n{name} x;y\n\n  \n{path}\r\n")).toEqual([
      "--name",
      "{name} x;y",
      "{path}",
    ]);
    expect(argsFromText(argsText(["a b", "c"]))).toEqual(["a b", "c"]);
  });

  it("needs a program", () => {
    expect(commandProblem("  ")).not.toBeNull();
    expect(commandProblem("scan.exe")).toBeNull();
  });
});

describe("the power row", () => {
  it("is titled by its action", () => {
    expect(powerTitle("shutdown")).toBe("Shut down when everything is done");
    expect(powerTitle("sleep")).toBe("Sleep when everything is done");
    expect(powerTitle("hibernate")).toBe("Hibernate when everything is done");
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
