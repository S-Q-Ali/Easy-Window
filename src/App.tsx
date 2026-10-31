import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type AppInfo = {
  name: string;
  version: string;
};

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    invoke<AppInfo>("app_info").then(setInfo).catch(console.error);
  }, []);

  return (
    <main className="app">
      <header className="app__header">
        <h1>Easy Window</h1>
        <p className="app__tagline">
          Know what is still logged in, and what is still running.
        </p>
      </header>

      <section className="app__modules">
        <article className="module">
          <h2>Chrome Session Guard</h2>
          <p>
            Closing a tab is not a logout. This module finds the sites that
            still hold a live session and clears them for real.
          </p>
          <span className="module__status">Not connected yet</span>
        </article>

        <article className="module">
          <h2>Windows Performance Guard</h2>
          <p>
            Finds background processes wasting CPU, memory, disk, and GPU, and
            explains which ones are safe to close.
          </p>
          <span className="module__status">Not sampling yet</span>
        </article>
      </section>

      <footer className="app__footer">
        {info ? `${info.name} v${info.version}` : "Connecting to Rust core…"}
      </footer>
    </main>
  );
}
