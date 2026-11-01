// Pure parsing for drag-and-drop / paste: pick a downloadable URL out of text
// handed over by the browser or clipboard. No download logic — the result only
// pre-fills the Add dialog.

/**
 * The first http(s) URL in `text`, or null. Accepts `text/uri-list` (one URI
 * per line, `#` lines are comments) and plain text containing a bare URL.
 */
export function extractHttpUrl(text: string | null | undefined): string | null {
  if (!text) return null;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    if (isHttpUrl(line)) return line;
  }
  return null;
}

/** True when `s` parses as an absolute http:// or https:// URL with a host. */
export function isHttpUrl(s: string): boolean {
  if (/\s/.test(s)) return false;
  try {
    const u = new URL(s);
    return (u.protocol === "http:" || u.protocol === "https:") && u.host !== "";
  } catch {
    return false;
  }
}

/** The first http(s) URL in a drop/paste payload, preferring `text/uri-list`. */
export function urlFromTransfer(data: Pick<DataTransfer, "getData"> | null): string | null {
  if (!data) return null;
  return (
    extractHttpUrl(data.getData("text/uri-list")) ?? extractHttpUrl(data.getData("text/plain"))
  );
}

/** Whether a drag carries text we might read a URL from (not just files). */
export function dragMayCarryUrl(types: readonly string[]): boolean {
  return types.includes("text/uri-list") || types.includes("text/plain");
}
