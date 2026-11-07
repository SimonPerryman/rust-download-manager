// Pure presentation helpers: turn the snapshot's raw numbers into the strings
// the mockup shows. This is formatting, not download logic.

/** Human-readable byte size, e.g. `1.01 GB`, `8.6 MB`. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i += 1;
  }
  return `${value < 10 ? value.toFixed(2) : value.toFixed(1)} ${units[i]}`;
}

/** Throughput string, e.g. `11.4 MB/s`. */
export function formatSpeed(bytesPerSec: number): string {
  return `${formatBytes(Math.round(bytesPerSec))}/s`;
}

/** The status bar's activity line, e.g. "2 downloading, 1 waiting". */
export function statusCountsLine(downloading: number, waiting: number): string {
  const parts = [];
  if (downloading > 0) parts.push(`${downloading} downloading`);
  if (waiting > 0) parts.push(`${waiting} waiting`);
  return parts.length ? parts.join(", ") : "Nothing downloading";
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** The Added column, in local time: "14:05" today, "Yesterday", else "3 Nov"; blank when unknown. */
export function formatAdded(epochSecs: number | null, now: Date = new Date()): string {
  if (epochSecs == null) return "";
  const at = new Date(epochSecs * 1000);
  const today = startOfDay(now);
  if (at.getTime() >= today) {
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${pad(at.getHours())}:${pad(at.getMinutes())}`;
  }
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1).getTime();
  if (at.getTime() >= yesterday) return "Yesterday";
  return `${at.getDate()} ${MONTHS[at.getMonth()]}`;
}

/** The Time left column, e.g. "9 s", "12 min", "1 h 5 min"; blank when unknown. */
export function formatLeft(secs: number | null): string {
  if (secs == null) return "";
  const s = Math.max(0, Math.floor(secs));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return m ? `${h} h ${m} min` : `${h} h`;
}

/** The URL's host, e.g. `releases.ubuntu.com`, which the name filter also matches. */
export function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
