// Plain-words copy for a failed download's inline error: what happened and
// what to do. The raw engine text stays available for the row's tooltip.

const REASONS: Record<number, string> = {
  401: "Unauthorized",
  403: "Forbidden",
  404: "Not Found",
  410: "Gone",
  429: "Too Many Requests",
  500: "Internal Server Error",
  502: "Bad Gateway",
  503: "Service Unavailable",
  504: "Gateway Timeout",
};

function describeHttp(code: number): string {
  const tag = REASONS[code] ? `${code} ${REASONS[code]}` : `${code}`;
  if (code === 401 || code === 403) {
    return `The server refused the request (${tag}). The link may have expired. Copy a fresh link and retry.`;
  }
  if (code === 404 || code === 410) {
    return `The file isn't at this link any more (${tag}). Check the link and retry.`;
  }
  if (code === 429) return `The server is limiting requests (${tag}). Wait a minute, then retry.`;
  if (code >= 500 && code < 600) return `The server had a problem (${tag}). Retry in a moment.`;
  return `The server answered with an error (${tag}). Check the link and retry.`;
}

// Checked in order: a DNS failure is also reported as a failure to connect.
const TEXT_CLASSES: [RegExp, string][] = [
  [/timed out|timeout/i, "The server stopped responding. Check your connection, then retry."],
  [/dns|resolve|lookup/i, "Couldn't find the server. Check the link, then retry."],
  [/refused|reset|connect/i, "Couldn't reach the server. Check your connection, then retry."],
  [/no space|os error 28\b|os error 112\b/i, "The disk is full. Free some space, then retry."],
];

/** A failed download's error in plain words, ending with what to do. */
export function describeError(raw: string | null): string {
  const text = raw?.trim();
  if (!text) return "The download failed. Retry, or check the link.";
  const http = /HTTP (\d{3})/.exec(text);
  if (http) return describeHttp(Number(http[1]));
  // reqwest's text embeds the URL ("for url (https://…)"); a host or path such as
  // connect.example.org must not decide the class. The URL may hold ")", so it
  // ends at the error chain's ": " separator or the end of the text.
  const withoutUrl = text.replace(/\s*\bfor url \(.*?\)(?=: |$)/gi, "");
  for (const [pattern, copy] of TEXT_CLASSES) if (pattern.test(withoutUrl)) return copy;
  const plain = withoutUrl.trim() || text;
  const sentence = plain[0].toUpperCase() + plain.slice(1);
  return `${sentence}${sentence.endsWith(".") ? "" : "."} Retry, or check the link.`;
}
