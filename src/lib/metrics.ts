//! The wire shapes the sampler publishes, mirrored from `src-tauri/src/monitor`.
//!
//! These are declarations only. The Rust tests in `monitor/types.rs` are the
//! authority on the key names; if a field changes there, it changes here.

export type SignatureStatus = "trusted" | "unsigned" | "invalid" | "unknown";

export type ProcessSnapshot = {
  pid: number;
  parentPid: number | null;
  name: string;
  exePath: string | null;
  publisher: string | null;
  signature: SignatureStatus;
  signatureReason: string | null;
  sessionId: number | null;
  windowTitle: string | null;
  startTime: number;
  memoryBytes: number;
  cpuPercent: number | null;
  diskReadBytesPerSec: number | null;
  diskWriteBytesPerSec: number | null;
  criticalReason: string | null;
  selectable: boolean;
};

export type ProcessMetrics = {
  process: ProcessSnapshot;
  gpuPercent: number | null;
};

export type GpuTelemetry =
  | { kind: "warming" }
  | {
      kind: "sampling";
      enginesSampled: number;
      enginesUnsampled: number;
      unparsed: number;
    }
  | { kind: "unavailable"; reason: string };

export type SystemSummary = {
  cpuPercent: number | null;
  diskReadBytesPerSec: number | null;
  diskWriteBytesPerSec: number | null;
  gpuPercent: number | null;
  processCount: number;
  unmeasuredProcesses: number;
};

export type Tick = {
  sequence: number;
  processes: ProcessMetrics[];
  summary: SystemSummary;
  gpu: GpuTelemetry;
  gpuSequence: number | null;
};

export type SamplingState =
  | { state: "running"; tick: Tick | null }
  | { state: "failed"; reason: string };

/** A process is identified by its PID *and* when it started, never PID alone. */
export function rowKey(process: ProcessSnapshot): string {
  return `${process.pid}:${process.startTime}`;
}
