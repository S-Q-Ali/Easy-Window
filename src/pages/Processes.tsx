import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import { ProcessTable } from "../components/ProcessTable/ProcessTable";
import { ActivityIcon } from "../components/icons";
import { MetricHistory } from "../lib/history";
import type { SamplingState, Tick } from "../lib/metrics";
import "./pages.css";

export function Processes() {
  const [tick, setTick] = useState<Tick | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [history] = useState(() => new MetricHistory());
  const shown = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | null = null;

    // The pull and the push both land here, and ticks are numbered, so the
    // older of the two never wins — and the sparkline history, which is built
    // by sample count rather than by time, never rewinds or double-counts.
    const apply = (next: Tick) => {
      if (cancelled) return;
      if (shown.current !== null && next.sequence <= shown.current) return;
      shown.current = next.sequence;
      history.record(next);
      setTick(next);
    };

    invoke<SamplingState>("latest_tick")
      .then((state) => {
        if (cancelled) return;
        if (state.state === "failed") {
          setFailure(state.reason);
          return;
        }
        if (state.tick !== null) apply(state.tick);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setFailure(String(reason));
      });

    listen<Tick>("metrics://tick", (event) => apply(event.payload))
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch((reason: unknown) => {
        if (!cancelled) setFailure(String(reason));
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [history]);

  return (
    <div className="page">
      <PageHeader
        title="Processes"
        description="Per-process CPU, memory, disk, and GPU, with a safety verdict."
      />

      {failure !== null ? (
        <EmptyState title="Could not read the process monitor" icon={<ActivityIcon size={20} />}>
          <p>{failure}</p>
          <p>
            The rest of the app still works; only the live table is unavailable.
          </p>
        </EmptyState>
      ) : tick === null ? (
        <div role="status">
          <EmptyState title="Measuring the first tick" icon={<ActivityIcon size={20} />}>
            <p>
              The first read takes about ten seconds: every executable on the
              machine is signature-checked before it is listed.
            </p>
            <p>After that the table refreshes on every tick the sampler publishes.</p>
          </EmptyState>
        </div>
      ) : (
        <ProcessTable tick={tick} history={history} />
      )}
    </div>
  );
}
