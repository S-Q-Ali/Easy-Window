import { EmptyState } from "../components/EmptyState";
import { PageHeader } from "../components/PageHeader";
import { ShieldIcon } from "../components/icons";
import "./pages.css";

export function Sessions() {
  return (
    <div className="page">
      <PageHeader
        title="Sessions"
        description="Websites that still hold a login session in Chrome."
      />

      <EmptyState title="No session scan yet" icon={<ShieldIcon size={20} />}>
        <p>
          The Chrome extension is not installed, so Easy Window cannot see which
          sites are still logged in.
        </p>
        <p>
          Once connected, this page lists every site with its risk reasons and a
          one-click logout that clears cookies, local storage, IndexedDB, and
          service workers for that site.
        </p>
      </EmptyState>
    </div>
  );
}
