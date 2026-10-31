import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import { PowerIcon } from "../components/icons";
import "./pages.css";

export function Startup() {
  return (
    <div className="page">
      <PageHeader
        title="Startup"
        description="What launches with Windows, and what it costs you at login."
      />

      <EmptyState title="No startup audit yet" icon={<PowerIcon size={20} />}>
        <p>
          This build does not read the startup locations yet, so the list is
          empty rather than wrong.
        </p>
        <p>
          The audit will cover your Run keys, the Startup folder, and user-level
          scheduled tasks — with a reversible disable and a backup of the
          original value. Windows services are out of scope.
        </p>
      </EmptyState>
    </div>
  );
}
