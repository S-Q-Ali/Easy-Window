import { PageHeader } from "../components/PageHeader";
import { ActivityIcon, ShieldIcon } from "../components/icons";
import "./pages.css";

export function Overview() {
  return (
    <div className="page">
      <PageHeader
        title="Overview"
        description="What is still logged in, and what is still running on this machine."
      />

      <p className="page__note">
        Setup is not finished. Both modules are read-only placeholders until the
        telemetry and extension slices land.
      </p>

      <ul className="module-list">
        <li className="module-row">
          <span className="module-row__icon">
            <ShieldIcon size={18} />
          </span>
          <div className="module-row__body">
            <h2>Chrome Session Guard</h2>
            <p>
              Finds sites that still hold a live login session and clears the
              real session data, not just the tab.
            </p>
          </div>
          <span className="state">Not connected</span>
        </li>

        <li className="module-row">
          <span className="module-row__icon">
            <ActivityIcon size={18} />
          </span>
          <div className="module-row__body">
            <h2>Windows Performance Guard</h2>
            <p>
              Finds background processes wasting CPU, memory, disk, or GPU, and
              explains which ones are safe to close.
            </p>
          </div>
          <span className="state">Not sampling</span>
        </li>
      </ul>
    </div>
  );
}
