import type { ProcessMetrics } from "../../lib/metrics";

export type SortColumn = "name" | "cpu" | "memory" | "diskRead" | "diskWrite" | "gpu";
export type SortDirection = "ascending" | "descending";
export type Sort = { column: SortColumn; direction: SortDirection };

export const DEFAULT_SORT: Sort = { column: "cpu", direction: "descending" };

function metric(metrics: ProcessMetrics, column: Exclude<SortColumn, "name">): number | null {
  switch (column) {
    case "cpu":
      return metrics.process.cpuPercent;
    case "memory":
      return metrics.process.memoryBytes;
    case "diskRead":
      return metrics.process.diskReadBytesPerSec;
    case "diskWrite":
      return metrics.process.diskWriteBytesPerSec;
    case "gpu":
      return metrics.gpuPercent;
  }
}

function byName(a: ProcessMetrics, b: ProcessMetrics): number {
  const left = a.process.name.toLowerCase();
  const right = b.process.name.toLowerCase();
  if (left === right) return a.process.pid - b.process.pid;
  return left < right ? -1 : 1;
}

export function compareRows(sort: Sort): (a: ProcessMetrics, b: ProcessMetrics) => number {
  const direction = sort.direction === "ascending" ? 1 : -1;

  if (sort.column === "name") {
    return (a, b) => {
      const order = byName(a, b);
      return order * direction;
    };
  }

  const column = sort.column;
  return (a, b) => {
    const left = metric(a, column);
    const right = metric(b, column);

    // An unmeasured figure is not a small one, so it stays at the end whichever
    // way the column runs.
    if (left === null || right === null) {
      if (left === right) return byName(a, b);
      return left === null ? 1 : -1;
    }
    if (left !== right) return (left < right ? -1 : 1) * direction;
    return byName(a, b);
  };
}
