import { PAGES, type PageId } from "../../lib/navigation";
import {
  ActivityIcon,
  ListIcon,
  OverviewIcon,
  PowerIcon,
  ShieldIcon,
  SlidersIcon,
} from "../icons";
import "./Sidebar.css";

type SidebarProps = {
  page: PageId;
  onNavigate: (page: PageId) => void;
  version: string | null;
  offline: boolean;
};

function PageIcon({ id }: { id: PageId }) {
  switch (id) {
    case "overview":
      return <OverviewIcon />;
    case "sessions":
      return <ShieldIcon />;
    case "processes":
      return <ActivityIcon />;
    case "startup":
      return <PowerIcon />;
    case "settings":
      return <SlidersIcon />;
    case "audit":
      return <ListIcon />;
  }
}

export function Sidebar({ page, onNavigate, version, offline }: SidebarProps) {
  const statusLabel = offline
    ? "Core offline"
    : version
      ? "Core connected"
      : "Connecting…";

  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <svg
          width="22"
          height="22"
          viewBox="0 0 24 24"
          fill="none"
          aria-hidden="true"
          focusable="false"
        >
          <rect
            x="2.75"
            y="3.75"
            width="18.5"
            height="16.5"
            rx="3"
            stroke="currentColor"
            strokeWidth="1.6"
          />
          <path d="M2.75 8.5h18.5" stroke="currentColor" strokeWidth="1.6" />
          <circle cx="6" cy="6.1" r="0.85" fill="currentColor" />
          <circle cx="8.8" cy="6.1" r="0.85" fill="currentColor" />
        </svg>
        <span className="sidebar__brand-name">Easy Window</span>
      </div>

      <nav aria-label="Sections">
        <ul className="sidebar__nav">
          {PAGES.map((item) => (
            <li key={item.id}>
              <button
                type="button"
                className="sidebar__link"
                aria-current={page === item.id ? "page" : undefined}
                onClick={() => onNavigate(item.id)}
              >
                <PageIcon id={item.id} />
                {item.label}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      <div className="sidebar__footer">
        <span className="sidebar__status">
          <span
            className={
              "status-dot" +
              (offline ? " status-dot--error" : version ? " status-dot--ok" : "")
            }
            aria-hidden="true"
          />
          {statusLabel}
        </span>
        {version && <span className="sidebar__version">v{version}</span>}
      </div>
    </aside>
  );
}
