export type PageId =
  | "overview"
  | "sessions"
  | "processes"
  | "startup"
  | "settings"
  | "audit";

export type PageDef = {
  id: PageId;
  label: string;
};

export const PAGES: readonly PageDef[] = [
  { id: "overview", label: "Overview" },
  { id: "sessions", label: "Sessions" },
  { id: "processes", label: "Processes" },
  { id: "startup", label: "Startup" },
  { id: "settings", label: "Settings" },
  { id: "audit", label: "Audit" },
];
