import { describe, expect, it } from "vitest";
import { formatBytes, formatBytesPerSec, formatPercent, UNMEASURED } from "./format";

describe("formatPercent", () => {
  it("keeps an unmeasured figure out of the number space", () => {
    expect(formatPercent(null)).toBe(UNMEASURED);
    expect(formatPercent(null)).not.toBe(formatPercent(0));
  });

  it("shows one decimal so a column lines up", () => {
    expect(formatPercent(12.54)).toBe("12.5%");
    expect(formatPercent(0)).toBe("0.0%");
    expect(formatPercent(100)).toBe("100.0%");
  });
});

describe("formatBytesPerSec", () => {
  it("keeps an unmeasured rate an em dash", () => {
    expect(formatBytesPerSec(null)).toBe(UNMEASURED);
  });

  it("counts whole bytes below a kibibyte and one decimal above", () => {
    expect(formatBytesPerSec(0)).toBe("0 B/s");
    expect(formatBytesPerSec(512)).toBe("512 B/s");
    expect(formatBytesPerSec(1024)).toBe("1.0 KB/s");
    expect(formatBytesPerSec(5 * 1024 * 1024)).toBe("5.0 MB/s");
    expect(formatBytesPerSec(3.5 * 1024 ** 3)).toBe("3.5 GB/s");
  });
});

describe("formatBytes", () => {
  it("formats a size without a per-second unit", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
  });
});
