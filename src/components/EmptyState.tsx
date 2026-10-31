import type { ReactNode } from "react";
import "./EmptyState.css";

type EmptyStateProps = {
  title: string;
  children: ReactNode;
  icon?: ReactNode;
};

export function EmptyState({ title, children, icon }: EmptyStateProps) {
  return (
    <div className="empty-state">
      {icon ? (
        <div className="empty-state__icon" aria-hidden="true">
          {icon}
        </div>
      ) : null}
      <h2>{title}</h2>
      <div className="empty-state__body">{children}</div>
    </div>
  );
}
