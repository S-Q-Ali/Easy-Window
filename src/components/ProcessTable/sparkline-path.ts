//! The geometry of one row's sparkline: samples to SVG path data, nothing else.
//!
//! Kept apart from the component so the arithmetic is testable without a DOM,
//! the same split `sort.ts` and `format.ts` already use.

import { SAMPLE_HISTORY, type Series } from "../../lib/history";

export type SparklineSize = { width: number; height: number };

/** Keeps a 1 px stroke off the edge of the box. */
const INSET = 1;

function coordinate(value: number): string {
  return Number(value.toFixed(2)).toString();
}

/**
 * One path per measured run, so a sample the machine would not give is a gap
 * and not a line drawn down to zero. `ceiling` fixes the axis — CPU on 0–100 so
 * two rows are comparable — while a byte metric scales to its own window.
 */
export function sparklinePath(
  values: Series,
  size: SparklineSize,
  ceiling?: number,
): string[] {
  const measured = values.filter((value): value is number => value !== null);
  if (measured.length < 2) return [];

  const step = size.width / (SAMPLE_HISTORY - 1);
  const floor = size.height - INSET;
  const span = size.height - INSET * 2;
  const max = ceiling ?? Math.max(...measured);
  const right = values.length - 1;

  // A value over the ceiling is real — a per-PID GPU total is a sum over
  // engines — but a line that leaves the box reads as a rendering bug.
  const y = (value: number) =>
    max > 0 ? floor - Math.min(value / max, 1) * span : floor;

  const runs: string[] = [];
  let run: string[] = [];

  values.forEach((value, index) => {
    if (value === null) {
      if (run.length > 1) runs.push(`M${run.join(" L")}`);
      run = [];
      return;
    }
    run.push(
      `${coordinate(size.width - (right - index) * step)},${coordinate(y(value))}`,
    );
  });
  if (run.length > 1) runs.push(`M${run.join(" L")}`);

  return runs;
}
