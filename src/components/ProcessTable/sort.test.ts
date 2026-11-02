import { describe, expect, it } from "vitest";
import { row } from "./fixtures";
import { compareRows, type Sort } from "./sort";

const rows = [
  row({ pid: 1, name: "alpha", cpuPercent: 5, gpuPercent: 2, memoryBytes: 4096 }),
  row({ pid: 2, name: "bravo", cpuPercent: 40, gpuPercent: 0, memoryBytes: 1024 }),
  row({ pid: 3, name: "charlie", cpuPercent: null, memoryBytes: 8192 }),
  row({ pid: 4, name: "delta", cpuPercent: 12.5, gpuPercent: 30, memoryBytes: 2048 }),
];

function order(sort: Sort): string[] {
  return [...rows]
    .sort(compareRows(sort))
    .map((metrics) => metrics.process.name);
}

describe("compareRows", () => {
  it("runs a numeric column from the largest down", () => {
    expect(order({ column: "cpu", direction: "descending" })).toEqual([
      "bravo",
      "delta",
      "alpha",
      "charlie",
    ]);
  });

  it("runs the same column from the smallest up", () => {
    expect(order({ column: "cpu", direction: "ascending" })).toEqual([
      "alpha",
      "delta",
      "bravo",
      "charlie",
    ]);
  });

  it("leaves an unmeasured row at the end whichever way the column runs", () => {
    const disk = [
      row({ pid: 1, name: "slow", diskReadBytesPerSec: 10 }),
      row({ pid: 2, name: "fast", diskReadBytesPerSec: 900 }),
      row({ pid: 3, name: "blind" }),
    ];
    for (const direction of ["ascending", "descending"] as const) {
      const ordered = [...disk]
        .sort(compareRows({ column: "diskRead", direction }))
        .map((metrics) => metrics.process.name);
      expect(ordered[ordered.length - 1]).toBe("blind");
    }
  });

  it("sorts the GPU column on that process's own total", () => {
    expect(order({ column: "gpu", direction: "descending" })).toEqual([
      "delta",
      "alpha",
      "bravo",
      "charlie",
    ]);
  });

  it("sorts the two disk rates as separate columns", () => {
    const read = [
      row({ pid: 1, name: "reader", diskReadBytesPerSec: 99 }),
      row({ pid: 2, name: "writer", diskWriteBytesPerSec: 99 }),
    ];
    expect(
      [...read]
        .sort(compareRows({ column: "diskWrite", direction: "descending" }))
        .map((metrics) => metrics.process.name),
    ).toEqual(["writer", "reader"]);
  });

  it("compares names without caring about case", () => {
    const names = [
      row({ pid: 1, name: "Zed" }),
      row({ pid: 2, name: "apple" }),
    ];
    expect(
      [...names]
        .sort(compareRows({ column: "name", direction: "ascending" }))
        .map((metrics) => metrics.process.name),
    ).toEqual(["apple", "Zed"]);
  });

  it("breaks a tie by name so rows do not shuffle from tick to tick", () => {
    const tied = [
      row({ pid: 1, name: "romeo", cpuPercent: 3 }),
      row({ pid: 2, name: "alice", cpuPercent: 3 }),
    ];
    for (const direction of ["ascending", "descending"] as const) {
      expect(
        [...tied]
          .sort(compareRows({ column: "cpu", direction }))
          .map((metrics) => metrics.process.name),
      ).toEqual(["alice", "romeo"]);
    }
  });
});
