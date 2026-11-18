// Seeds start-at schedules whose time passed "while rdm was closed", for the missed-starts spec.
// The engine never fires a past start by itself at launch, and the only way to get one is a state
// file written before the app starts, so the config calls this just before that spec's session.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

/** A Monday long ago, so every one of them is past and reads "Was due 6 Jan 22:00". */
const DUE = "2020-01-06T22:00";

interface Seeded {
  version: number;
  next_id: number;
  downloads: Record<string, unknown>[];
  settings: Record<string, unknown>;
}

/**
 * Adds four missed schedules to `<home>/state.json`: a batch "Season 2" of two files and three
 * single downloads (six downloads), all waiting for the user.
 */
export function seedMissedStarts(home: string, baseUrl: string, downloadDir: string): void {
  const file = path.join(home, "state.json");
  const state: Seeded = existsSync(file)
    ? JSON.parse(readFileSync(file, "utf8"))
    : { version: 3, next_id: 1, downloads: [], settings: {} };

  const add = (name: string, group: number, extra: Record<string, unknown> = {}) => {
    const id = state.next_id++;
    state.downloads.push({
      id,
      url: `${baseUrl}/file?missed=${name}`,
      target: path.join(downloadDir, `${name}.bin`),
      status: "queued",
      start_at: DUE,
      start_group: group,
      ...extra,
    });
    return id;
  };

  const first = state.next_id;
  const batch = { batch: first, batch_label: "Season 2" };
  add("season-2-a", 9001, batch);
  add("season-2-b", 9001, batch);
  add("missed-one", 9002);
  add("missed-two", 9003);
  add("missed-three", 9004);
  writeFileSync(file, JSON.stringify(state));
}
