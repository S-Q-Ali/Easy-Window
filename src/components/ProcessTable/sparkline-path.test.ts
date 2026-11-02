import { describe, expect, it } from "vitest";
import { SAMPLE_HISTORY } from "../../lib/history";
import { sparklinePath } from "./sparkline-path";

type Point = { x: number; y: number };

/** Flattens every run back into points so the assertions are about geometry. */
function points(runs: string[]): Point[] {
  return runs.flatMap((run) =>
    (run.match(/-?\d+(?:\.\d+)?/g) ?? [])
      .map(Number)
      .reduce<Point[]>((acc, value, index) => {
        if (index % 2 === 0) acc.push({ x: value, y: 0 });
        else acc[acc.length - 1].y = value;
        return acc;
      }, []),
  );
}

const SIZE = { width: 48, height: 14 };
const step = SIZE.width / (SAMPLE_HISTORY - 1);

describe("sparklinePath", () => {
  it("draws nothing until there are two samples to join", () => {
    expect(sparklinePath([], SIZE)).toEqual([]);
    expect(sparklinePath([12], SIZE)).toEqual([]);
  });

  it("puts the newest sample on the right edge and grows leftwards", () => {
    const drawn = points(sparklinePath([10, 20, 30], SIZE));

    expect(drawn).toHaveLength(3);
    expect(drawn[2].x).toBeCloseTo(SIZE.width, 2);
    expect(drawn[1].x).toBeCloseTo(SIZE.width - step, 2);
    expect(drawn[0].x).toBeCloseTo(SIZE.width - 2 * step, 2);
  });

  it("keeps a young row short instead of stretching it across the axis", () => {
    const two = points(sparklinePath([10, 20], SIZE));
    const full = points(
      sparklinePath(Array.from({ length: SAMPLE_HISTORY }, (_, i) => i), SIZE),
    );

    // Samples sit in fixed slots, so two of them are one step apart at the right
    // edge; the line only reaches the left edge once the window is full.
    expect(two[0].x).toBeCloseTo(SIZE.width - step, 2);
    expect(two[1].x).toBeCloseTo(SIZE.width, 2);
    expect(full).toHaveLength(SAMPLE_HISTORY);
    expect(full[0].x).toBeCloseTo(0, 2);
  });

  it("breaks the line where a sample was not measured", () => {
    const runs = sparklinePath([10, 20, null, 30, 40], SIZE);

    expect(runs).toHaveLength(2);
    expect(runs[0]).toMatch(/^M/);
    expect(runs[1]).toMatch(/^M/);
    expect(points(runs)).toHaveLength(4);
  });

  it("puts a percentage on a fixed axis so two rows are comparable", () => {
    const half = points(sparklinePath([50, 50], SIZE, 100))[1];
    const quarter = points(sparklinePath([25, 25], SIZE, 100))[1];

    expect(half.y).toBeCloseTo(SIZE.height / 2, 2);
    expect(quarter.y).toBeGreaterThan(half.y);
  });

  it("pins a full percentage to the top and none to the bottom", () => {
    const [top, bottom] = [100, 0].map(
      (value) => points(sparklinePath([value, value], SIZE, 100))[1].y,
    );

    expect(top).toBeLessThan(SIZE.height / 2);
    expect(bottom).toBeGreaterThan(SIZE.height / 2);
  });

  it("scales a byte metric to its own window when it has no fixed axis", () => {
    const drawn = points(sparklinePath([0, 500, 1000], SIZE));

    expect(drawn[2].y).toBeLessThan(drawn[1].y);
    expect(drawn[1].y).toBeLessThan(drawn[0].y);
  });

  it("puts an all-idle window on the same floor as a zero sample", () => {
    const idle = points(sparklinePath([0, 0, 0], SIZE));
    const busy = points(sparklinePath([0, 10, 0], SIZE));
    const floor = Math.max(...busy.map((point) => point.y));

    expect(idle.map((point) => point.y)).toEqual(busy.map(() => floor));
  });

  it("flattens the quiet samples when the window holds a spike", () => {
    const spiked = points(sparklinePath([1, 1, 1, 1_000_000], SIZE));
    const still = points(sparklinePath([0, 0, 0, 0], SIZE));

    // Honest record of a max-scaled axis: 1 sits on the floor next to a million.
    expect(Math.abs(spiked[0].y - still[0].y)).toBeLessThan(0.01);
    expect(spiked[3].y).toBeLessThan(spiked[0].y);
  });
});
