// Locate…, its same-folder offer and Locate folder…: each runs the engine command and says what
// to show. The engine decides every match; the webview only passes ids and answers yes or no.

import * as api from "./api";
import { locateFolderMessage, relinkedMessage } from "./copy";
import { isMissing } from "./filters";
import type { DownloadSnapshot, SameFolderOffer } from "./types";

/** What to show after Locate… on `d`: a toast if it relinked, and any offer for the others. */
export async function locateOne(
  d: DownloadSnapshot,
): Promise<{ message: string | null; offer: SameFolderOffer | null }> {
  const outcome = await api.locateDownload(d.id);
  return {
    message:
      outcome.relinked && outcome.new_path
        ? relinkedMessage(d.final_path ?? d.filename, outcome.new_path)
        : null,
    offer: outcome.relinked ? outcome.offer : null,
  };
}

/** What Locate folder… did: the toast, and the ids of the downloads not in the folder. */
export interface FolderResult {
  message: string;
  /** Empty unless some were found, so the toast offers Show not found only on a partial result. */
  notFoundIds: number[];
}

/**
 * Locate folder… for the missing rows among `rows`: one folder picked, each row's same-named,
 * same-sized file looked for in it. Resolves with the result, or null if nothing was asked or
 * the picker was cancelled.
 */
export async function locateFolderFor(
  rows: readonly DownloadSnapshot[],
): Promise<FolderResult | null> {
  const ids = rows.filter(isMissing).map((d) => d.id);
  if (!ids.length) return null;
  const outcome = await api.locateFolder(ids);
  if (!outcome) return null;
  const notFound = outcome.not_found;
  return {
    message: locateFolderMessage(outcome.relinked, notFound.length),
    notFoundIds: outcome.relinked > 0 ? notFound : [],
  };
}
