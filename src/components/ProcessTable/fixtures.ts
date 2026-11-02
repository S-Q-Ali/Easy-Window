import type { GpuTelemetry, ProcessMetrics, Tick } from "../../lib/metrics";

export type RowSeed = {
  pid: number;
  name: string;
  startTime?: number;
  memoryBytes?: number;
  cpuPercent?: number | null;
  diskReadBytesPerSec?: number | null;
  diskWriteBytesPerSec?: number | null;
  gpuPercent?: number | null;
};

export function row(seed: RowSeed): ProcessMetrics {
  return {
    process: {
      pid: seed.pid,
      parentPid: null,
      name: seed.name,
      exePath: null,
      publisher: null,
      signature: "unknown",
      signatureReason: null,
      sessionId: 1,
      windowTitle: null,
      startTime: seed.startTime ?? 0,
      memoryBytes: seed.memoryBytes ?? 0,
      cpuPercent: seed.cpuPercent ?? null,
      diskReadBytesPerSec: seed.diskReadBytesPerSec ?? null,
      diskWriteBytesPerSec: seed.diskWriteBytesPerSec ?? null,
      criticalReason: null,
      selectable: true,
    },
    gpuPercent: seed.gpuPercent ?? null,
  };
}

export type TickSeed = {
  sequence?: number;
  gpu?: GpuTelemetry;
  gpuSequence?: number | null;
};

export function tick(rows: ProcessMetrics[], seed: TickSeed = {}): Tick {
  return {
    sequence: seed.sequence ?? 1,
    processes: rows,
    summary: {
      cpuPercent: null,
      diskReadBytesPerSec: null,
      diskWriteBytesPerSec: null,
      gpuPercent: null,
      processCount: rows.length,
      unmeasuredProcesses: rows.filter(
        (metrics) => metrics.process.cpuPercent === null,
      ).length,
    },
    gpu: seed.gpu ?? { kind: "warming" },
    gpuSequence: seed.gpuSequence ?? null,
  };
}
