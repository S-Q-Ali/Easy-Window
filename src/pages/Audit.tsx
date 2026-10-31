import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import { ListIcon } from "../components/icons";
import "./pages.css";

type AuditModule = "sessions" | "processes" | "startup" | "app";
type AuditOutcome = "success" | "partial" | "failed" | "blocked";

type AuditEvent = {
  id: number;
  occurred_at: number;
  module: AuditModule;
  action: string;
  target: string;
  outcome: AuditOutcome;
  detail: string | null;
  reversible: boolean;
};

const OUTCOME_LABEL: Record<AuditOutcome, string> = {
  success: "Done",
  partial: "Partial",
  failed: "Failed",
  blocked: "Blocked",
};

function formatTime(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString();
}

export function Audit() {
  const [events, setEvents] = useState<AuditEvent[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    invoke<AuditEvent[]>("list_audit_events", { limit: 100 })
      .then((rows) => {
        if (!cancelled) setEvents(rows);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(String(reason));
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="page">
      <PageHeader
        title="Audit"
        description="Every action Easy Window took, and what happened."
      />

      {error ? (
        <EmptyState title="Could not read the audit log" icon={<ListIcon size={20} />}>
          <p>{error}</p>
          <p>
            The rest of the app still works, but nothing can be recorded until
            the local store opens again.
          </p>
        </EmptyState>
      ) : events === null ? (
        <p className="page__muted" role="status">
          Reading the local log…
        </p>
      ) : events.length === 0 ? (
        <EmptyState title="No actions recorded" icon={<ListIcon size={20} />}>
          <p>Nothing has been changed yet, so the log is empty.</p>
          <p>
            Every logout, close, and startup change will be written here with a
            timestamp and a result. Cookie values, page content, and process
            command lines are never recorded.
          </p>
        </EmptyState>
      ) : (
        <ol className="audit-list">
          {events.map((event) => (
            <li key={event.id} className="audit-row">
              <time className="audit-row__time" dateTime={new Date(event.occurred_at * 1000).toISOString()}>
                {formatTime(event.occurred_at)}
              </time>
              <span className="audit-row__action">
                {event.action.replace(/_/g, " ")}
              </span>
              <span className="audit-row__target">{event.target}</span>
              <span className={`state state--${event.outcome}`}>
                {OUTCOME_LABEL[event.outcome]}
              </span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
