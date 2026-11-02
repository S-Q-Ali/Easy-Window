import { useMemo, useState } from "react";
import { MetricHistory, type Series } from "../../lib/history";
import { rowKey, type ProcessMetrics, type Tick } from "../../lib/metrics";
import { formatBytes, formatBytesPerSec, formatPercent } from "./format";
import { Sparkline } from "./Sparkline";
import { compareRows, DEFAULT_SORT, type Sort, type SortColumn } from "./sort";
import "./ProcessTable.css";

type Header = { label: string; column: SortColumn | null };

const HEADERS: Header[] = [
  { label: "Name", column: "name" },
  { label: "PID", column: null },
  { label: "CPU", column: "cpu" },
  { label: "Memory", column: "memory" },
  { label: "Disk read", column: "diskRead" },
  { label: "Disk write", column: "diskWrite" },
  { label: "GPU", column: "gpu" },
];

function matchesQuery(metrics: ProcessMetrics, query: string): boolean {
  return (
    metrics.process.name.toLowerCase().includes(query) ||
    String(metrics.process.pid).includes(query)
  );
}

function processes(count: number): string {
  return `${count} ${count === 1 ? "process" : "processes"}`;
}

function gpuNote(tick: Tick): string | null {
  if (tick.gpu.kind === "warming") {
    return "GPU has no reading yet — the counter is still warming up.";
  }
  if (tick.gpu.kind === "unavailable") {
    return `GPU has no reading on this machine: ${tick.gpu.reason}.`;
  }
  return null;
}

function staleGpuTick(tick: Tick): number | null {
  if (tick.gpu.kind !== "sampling" || tick.gpuSequence === null) return null;
  return tick.gpuSequence < tick.sequence ? tick.gpuSequence : null;
}

type MetricCellProps = {
  value: number | null;
  series: Series;
  format: (value: number | null) => string;
  ceiling?: number;
  active: boolean;
};

function MetricCell({ value, series, format, ceiling, active }: MetricCellProps) {
  return (
    <td className="process-table__num">
      {/* One line per row, on the column being read. Drawing all five measured
          46 ms of geometry and 702 KB of path data per tick for shapes the
          reader is not looking at; one costs a fifth of that. */}
      {active ? <Sparkline values={series} ceiling={ceiling} /> : null}
      <span>{format(value)}</span>
    </td>
  );
}

export function ProcessTable({ tick, history }: { tick: Tick; history: MetricHistory }) {
  const [sort, setSort] = useState<Sort>(DEFAULT_SORT);
  const [query, setQuery] = useState("");

  const term = query.trim().toLowerCase();
  const visible = useMemo(
    () =>
      term === ""
        ? tick.processes
        : tick.processes.filter((metrics) => matchesQuery(metrics, term)),
    [tick.processes, term],
  );
  const sorted = useMemo(() => [...visible].sort(compareRows(sort)), [visible, sort]);

  const unmeasured = tick.summary.unmeasuredProcesses;
  const note = gpuNote(tick);
  const staleFrom = staleGpuTick(tick);

  function activate(column: SortColumn) {
    setSort((current) =>
      current.column === column
        ? {
            column,
            direction: current.direction === "ascending" ? "descending" : "ascending",
          }
        : { column, direction: "ascending" },
    );
  }

  return (
    <div className="process-table">
      <div className="process-table__toolbar">
        <div className="process-table__search">
          <label htmlFor="process-table-filter">Filter by name or PID</label>
          <input
            id="process-table-filter"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
        </div>
        <p className="process-table__count">
          {term === ""
            ? processes(visible.length)
            : `${visible.length} of ${processes(tick.processes.length)}`}
        </p>
      </div>

      <div className="process-table__notes">
        {unmeasured > 0 ? (
          <p className="process-table__note">
            {`${unmeasured} of ${processes(tick.processes.length)} report no CPU or disk figure.`}
          </p>
        ) : null}
        {note ? <p className="process-table__note">{note}</p> : null}
      </div>

      {visible.length === 0 ? (
        <div className="process-table__none">
          <p role="status">No process matches “{query.trim()}”.</p>
          <button type="button" onClick={() => setQuery("")}>
            Clear filter
          </button>
        </div>
      ) : (
        <div className="process-table__scroll">
          <table>
            <caption className="visually-hidden">
              Process metrics, tick {tick.sequence}.
            </caption>
            <thead>
              <tr>
                {HEADERS.map((header) => {
                  const column = header.column;
                  return (
                    <th
                      key={header.label}
                      scope="col"
                      aria-sort={
                        column === null
                          ? undefined
                          : sort.column === column
                            ? sort.direction
                            : "none"
                      }
                    >
                      {column === null ? (
                        header.label
                      ) : (
                        <button
                          type="button"
                          className="process-table__sort"
                          onClick={() => activate(column)}
                        >
                          {header.label}
                          {column === "gpu" && staleFrom !== null ? (
                            <span className="process-table__stale">
                              <span className="visually-hidden">
                                figures from tick {staleFrom}
                              </span>
                              <span aria-hidden="true">#{staleFrom}</span>
                            </span>
                          ) : null}
                        </button>
                      )}
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {sorted.map((metrics) => {
                const windows = history.seriesFor(metrics.process);
                const { process } = metrics;
                return (
                  <tr key={rowKey(process)}>
                    <td className="process-table__name">{process.name}</td>
                    <td className="process-table__num">{process.pid}</td>
                    {/* CPU has a fixed axis, so two rows are comparable at a glance. */}
                    <MetricCell
                      value={process.cpuPercent}
                      series={windows.cpu}
                      format={formatPercent}
                      ceiling={100}
                      active={sort.column === "cpu"}
                    />
                    <MetricCell
                      value={process.memoryBytes}
                      series={windows.memory}
                      format={formatBytes}
                      active={sort.column === "memory"}
                    />
                    <MetricCell
                      value={process.diskReadBytesPerSec}
                      series={windows.diskRead}
                      format={formatBytesPerSec}
                      active={sort.column === "diskRead"}
                    />
                    <MetricCell
                      value={process.diskWriteBytesPerSec}
                      series={windows.diskWrite}
                      format={formatBytesPerSec}
                      active={sort.column === "diskWrite"}
                    />
                    {/* No ceiling: a per-PID GPU total sums every engine and can
                        legitimately pass 100, which a fixed axis would clip away. */}
                    <MetricCell
                      value={metrics.gpuPercent}
                      series={windows.gpu}
                      format={formatPercent}
                      active={sort.column === "gpu"}
                    />
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
