import type { Series } from "../../lib/history";
import { sparklinePath } from "./sparkline-path";

const SIZE = { width: 48, height: 14 };

/**
 * The shape of the last 60 samples. The number beside it is the reading, so the
 * line carries no information a screen reader would need and is hidden from
 * assistive tech entirely.
 */
export function Sparkline({ values, ceiling }: { values: Series; ceiling?: number }) {
  const runs = sparklinePath(values, SIZE, ceiling);
  if (runs.length === 0) return null;

  return (
    <svg
      className="process-table__spark"
      width={SIZE.width}
      height={SIZE.height}
      viewBox={`0 0 ${SIZE.width} ${SIZE.height}`}
      aria-hidden="true"
      focusable="false"
    >
      {runs.map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}
