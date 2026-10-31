import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import { ListIcon } from "../components/icons";
import "./pages.css";

export function Audit() {
  return (
    <div className="page">
      <PageHeader
        title="Audit"
        description="Every action Easy Window took, and what happened."
      />

      <EmptyState title="No actions recorded" icon={<ListIcon size={20} />}>
        <p>
          Nothing has been changed yet, so the log is empty.
        </p>
        <p>
          Every logout, close, and startup change will be written here with a
          timestamp and a result. Cookie values, page content, and process
          command lines are never recorded.
        </p>
      </EmptyState>
    </div>
  );
}
