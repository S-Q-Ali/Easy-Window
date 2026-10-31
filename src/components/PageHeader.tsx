import type { ReactNode } from "react";
import "./PageHeader.css";

type PageHeaderProps = {
  title: string;
  description: string;
  children?: ReactNode;
};

export function PageHeader({ title, description, children }: PageHeaderProps) {
  return (
    <header className="page-header">
      <div className="page-header__text">
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      {children ? <div className="page-header__actions">{children}</div> : null}
    </header>
  );
}
