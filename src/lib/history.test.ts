import { describe, expect, it } from "vitest";
import { row, tick } from "../components/ProcessTable/fixtures";
import type { ProcessSnapshot } from "./metrics";
import { MetricHistory, SAMPLE_HISTORY, type MetricKey } from "./history";

const KEYS: MetricKey[] = ["cpu", "memory", "diskRead", "diskWrite", "gpu"];

const notepad = row({
  pid: 4712,
  name: "notepad.exe",
  startTime: 100,
  cpuPercent: 10,
  memoryBytes: 1024,
  diskReadBytesPerSec: 2048,
  diskWriteBytesPerSec: 4096,
  gpuPercent: 3,
});

function processOf(metrics: typeof notepad): ProcessSnapshot {
  return metrics.process;
}

function series(history: MetricHistory, metrics = notepad) {
  return history.seriesFor(processOf(metrics));
}

describe("MetricHistory", () => {
  it("keeps one sample per metric for every tick it is shown", () => {
    const history = new MetricHistory();
    history.record(tick([notepad]));
    history.record(tick([notepad], { sequence: 2 }));

    for (const key of KEYS) expect(series(history)[key]).toHaveLength(2);
    expect(series(history).cpu).toEqual([10, 10]);
    expect(series(history).memory).toEqual([1024, 1024]);
    expect(series(history).gpu).toEqual([3, 3]);
  });

  it("drops the oldest sample once the window is full", () => {
    const history = new MetricHistory();
    for (let sequence = 1; sequence <= SAMPLE_HISTORY + 5; sequence += 1) {
      history.record(
        tick(
          [row({ pid: 1, name: "a.exe", cpuPercent: sequence })],
          { sequence },
        ),
      );
    }

    const cpu = series(history, row({ pid: 1, name: "a.exe", cpuPercent: 1 })).cpu;
    expect(cpu).toHaveLength(SAMPLE_HISTORY);
    expect(cpu[0]).toBe(6);
    expect(cpu[cpu.length - 1]).toBe(SAMPLE_HISTORY + 5);
  });

  it("records a metric Windows would not give as a gap, never as zero", () => {
    const history = new MetricHistory();
    history.record(tick([row({ pid: 4, name: "svchost.exe" })]));

    const blind = series(history, row({ pid: 4, name: "svchost.exe" }));
    expect(blind.cpu).toEqual([null]);
    expect(blind.memory).toEqual([0]);
    expect(blind.diskRead).toEqual([null]);
    expect(blind.gpu).toEqual([null]);
  });

  it("gives a recycled PID no history at all", () => {
    const history = new MetricHistory();
    history.record(tick([notepad]));
    history.record(tick([notepad], { sequence: 2 }));

    const samePidNewProcess = row({
      pid: notepad.process.pid,
      name: "notepad.exe",
      startTime: 999,
      cpuPercent: 55,
    });
    history.record(tick([samePidNewProcess], { sequence: 3 }));

    expect(series(history, samePidNewProcess).cpu).toEqual([55]);
  });

  it("forgets a process that is not in the newest tick", () => {
    const history = new MetricHistory();
    history.record(tick([notepad]));

    const chrome = row({ pid: 8000, name: "chrome.exe", cpuPercent: 20 });
    history.record(tick([chrome], { sequence: 2 }));

    expect(series(history)).toEqual({
      cpu: [],
      memory: [],
      diskRead: [],
      diskWrite: [],
      gpu: [],
    });
    expect(series(history, chrome).cpu).toEqual([20]);
  });

  it("shares one window between processes that live in the same tick", () => {
    const history = new MetricHistory();
    const chrome = row({ pid: 8000, name: "chrome.exe", cpuPercent: 20 });
    history.record(tick([notepad, chrome]));

    expect(series(history).cpu).toEqual([10]);
    expect(series(history, chrome).cpu).toEqual([20]);
  });
});
