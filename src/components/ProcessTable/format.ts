const UNITS = ["B", "KB", "MB", "GB", "TB"];
const KIBIBYTE = 1024;

/** A metric the sampler could not read. Never `0`, which would read as idle. */
export const UNMEASURED = "—";

function formatSize(bytes: number, perSecond: boolean): string {
  let value = bytes;
  let unit = 0;
  while (value >= KIBIBYTE && unit < UNITS.length - 1) {
    value /= KIBIBYTE;
    unit += 1;
  }
  const label = perSecond ? `${UNITS[unit]}/s` : UNITS[unit];
  return unit === 0 ? `${Math.round(value)} ${label}` : `${value.toFixed(1)} ${label}`;
}

export function formatPercent(value: number | null): string {
  return value === null ? UNMEASURED : `${value.toFixed(1)}%`;
}

export function formatBytes(value: number): string {
  return formatSize(value, false);
}

export function formatBytesPerSec(value: number | null): string {
  return value === null ? UNMEASURED : formatSize(value, true);
}
