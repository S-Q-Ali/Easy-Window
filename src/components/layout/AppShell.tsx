import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PageId } from "../../lib/navigation";
import { Sidebar } from "./Sidebar";
import { Overview } from "../../pages/Overview";
import { Sessions } from "../../pages/Sessions";
import { Processes } from "../../pages/Processes";
import { Startup } from "../../pages/Startup";
import { Settings } from "../../pages/Settings";
import { Audit } from "../../pages/Audit";
import "./AppShell.css";

type AppInfo = {
  name: string;
  version: string;
};

export function AppShell() {
  const [page, setPage] = useState<PageId>("overview");
  const [version, setVersion] = useState<string | null>(null);
  const [offline, setOffline] = useState(false);

  useEffect(() => {
    let cancelled = false;

    invoke<AppInfo>("app_info")
      .then((info) => {
        if (!cancelled) setVersion(info.version);
      })
      .catch(() => {
        if (!cancelled) setOffline(true);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="shell">
      <a className="skip-link" href="#main-content">
        Skip to content
      </a>

      <Sidebar
        page={page}
        onNavigate={setPage}
        version={version}
        offline={offline}
      />

      <main className="shell__main" id="main-content" tabIndex={-1}>
        {page === "overview" && <Overview />}
        {page === "sessions" && <Sessions />}
        {page === "processes" && <Processes />}
        {page === "startup" && <Startup />}
        {page === "settings" && <Settings />}
        {page === "audit" && <Audit />}
      </main>
    </div>
  );
}
