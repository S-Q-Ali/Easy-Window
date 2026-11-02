//! Per-process metric history, built in the webview from the ticks it is shown.
//!
//! The x-axis is 60 *samples*, never 60 seconds: a tick carries no timestamp and
//! arrives anywhere between 42 ms and 4.1 s after the last one, so an age
//! computed from wall clock would be wrong by an order of magnitude.

import { rowKey, type ProcessSnapshot, type Tick } from "./metrics";

export const SAMPLE_HISTORY = 60;

export type MetricKey = "cpu" | "memory" | "diskRead" | "diskWrite" | "gpu";

/** Oldest first. A `null` is a sample the machine would not give, not a zero. */
export type Series = (number | null)[];

export type Windows = Record<MetricKey, Series>;

function emptyWindows(): Windows {
  return { cpu: [], memory: [], diskRead: [], diskWrite: [], gpu: [] };
}

function append(series: Series, value: number | null): void {
  series.push(value);
  if (series.length > SAMPLE_HISTORY) series.shift();
}

export class MetricHistory {
  private windows = new Map<string, Windows>();

  /**
   * Keeps only what the tick lists. A snapshot is the whole process list, so an
   * identity missing from it has exited and its history is worth nothing — that
   * is also what bounds this structure at one tick's worth of rows.
   */
  record(tick: Tick): void {
    const next = new Map<string, Windows>();

    for (const metrics of tick.processes) {
      const key = rowKey(metrics.process);
      const window = this.windows.get(key) ?? emptyWindows();
      append(window.cpu, metrics.process.cpuPercent);
      append(window.memory, metrics.process.memoryBytes);
      append(window.diskRead, metrics.process.diskReadBytesPerSec);
      append(window.diskWrite, metrics.process.diskWriteBytesPerSec);
      append(window.gpu, metrics.gpuPercent);
      next.set(key, window);
    }

    this.windows = next;
  }

  /** A process this has never seen reads as empty windows, not as `undefined`. */
  seriesFor(process: ProcessSnapshot): Windows {
    return this.windows.get(rowKey(process)) ?? emptyWindows();
  }
}
