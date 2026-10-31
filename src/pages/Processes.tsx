import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import { ActivityIcon } from "../components/icons";
import "./pages.css";

export function Processes() {
  return (
    <div className="page">
      <PageHeader
        title="Processes"
        description="Per-process CPU, memory, disk, and GPU, with a safety verdict."
      />

      <EmptyState title="Not sampling yet" icon={<ActivityIcon size={20} />}>
        <p>
          The Windows monitor is not wired up in this build, so there is nothing
          to measure yet.
        </p>
        <p>
          When it lands, this page shows the heaviest processes and flags the
          background apps that are safe to close. Critical Windows processes are
          never selectable.
        </p>
      </EmptyState>
    </div>
  );
}
