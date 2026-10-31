import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import { SlidersIcon } from "../components/icons";
import "./pages.css";

export function Settings() {
  return (
    <div className="page">
      <PageHeader
        title="Settings"
        description="Logout policy, sampling interval, retention, and appearance."
      />

      <EmptyState title="Nothing to configure yet" icon={<SlidersIcon size={20} />}>
        <p>
          Settings arrive with the features they control, so no switch here is a
          lie about what the app can do.
        </p>
        <p>
          The first ones will be the auto-logout policy, the idle threshold, and
          the sampling interval.
        </p>
      </EmptyState>
    </div>
  );
}
