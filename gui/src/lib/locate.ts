// Locate…, its same-folder offer and Locate folder…: each runs the engine command and says what
// to show. The engine decides every match; the webview only passes ids and answers yes or no.

import * as api from "./api";
import { locateFolderMessage, relinkedMessage, sameFolderAcceptedMessage } from "./copy";
import { isMissing } from "./filters";
import type { DownloadSnapshot, SameFolderOffer } from "./types";

/** What to show after Locate… on `d`: a toast if it relinked, and any offer for the others. */
export async function locateOne(
  d: DownloadSnapshot,
): Promise<{ message: string | null; offer: SameFolderOffer | null }> {
  const outcome = await api.locateDownload(d.id);
  return {
    message: outcome.relinked ? relinkedMessage(d) : null,
    offer: outcome.relinked ? outcome.offer : null,
  };
}

/** Accept the pending offer; resolves with the toast. */
export async function acceptOffer(): Promise<string> {
  return sameFolderAcceptedMessage(await api.acceptSameFolderOffer());
}

/**
 * Locate folder… for the missing rows among `rows`: one folder picked, each row's same-named,
 * same-sized file looked for in it. Resolves with the toast, or null if nothing was asked or
 * the picker was cancelled.
 */
export async function locateFolderFor(rows: readonly DownloadSnapshot[]): Promise<string | null> {
  const ids = rows.filter(isMissing).map((d) => d.id);
  if (!ids.length) return null;
  const outcome = await api.locateFolder(ids);
  return outcome && locateFolderMessage(outcome.relinked, outcome.not_found);
}
