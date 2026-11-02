import { useMemo, useState } from "react";
import { rowKey, type ProcessMetrics, type Tick } from "../../lib/metrics";
import { formatBytes, formatBytesPerSec, formatPercent } from "./format";
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

export function ProcessTable({ tick }: { tick: Tick }) {
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
              {sorted.map((metrics) => (
                <tr key={rowKey(metrics.process)}>
                  <td className="process-table__name">{metrics.process.name}</td>
                  <td className="process-table__num">{metrics.process.pid}</td>
                  <td className="process-table__num">
                    {formatPercent(metrics.process.cpuPercent)}
                  </td>
                  <td className="process-table__num">
                    {formatBytes(metrics.process.memoryBytes)}
                  </td>
                  <td className="process-table__num">
                    {formatBytesPerSec(metrics.process.diskReadBytesPerSec)}
                  </td>
                  <td className="process-table__num">
                    {formatBytesPerSec(metrics.process.diskWriteBytesPerSec)}
                  </td>
                  <td className="process-table__num">
                    {formatPercent(metrics.gpuPercent)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
