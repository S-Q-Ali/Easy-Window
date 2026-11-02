import axe from "axe-core";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MetricHistory } from "../../lib/history";
import type { Tick } from "../../lib/metrics";
import { row, tick } from "./fixtures";
import { ProcessTable } from "./ProcessTable";

const sampling = {
  kind: "sampling",
  enginesSampled: 307,
  enginesUnsampled: 0,
  unparsed: 0,
} as const;

const rows = [
  row({
    pid: 4712,
    name: "notepad.exe",
    cpuPercent: 12.5,
    memoryBytes: 20 * 1024 * 1024,
    diskReadBytesPerSec: 1024,
    diskWriteBytesPerSec: null,
    gpuPercent: 3,
  }),
  row({
    pid: 8000,
    name: "Chrome.exe",
    cpuPercent: 40,
    memoryBytes: 512 * 1024 * 1024,
    diskReadBytesPerSec: 0,
    diskWriteBytesPerSec: 4 * 1024 * 1024,
    gpuPercent: 0,
  }),
  row({ pid: 4, name: "svchost.exe", memoryBytes: 0 }),
];

function bodyRows(): HTMLElement[] {
  return screen.getAllByRole("row").filter((r) => r.querySelector("td") !== null);
}

function rowCells(name: string): HTMLTableCellElement[] {
  const bodyRow = bodyRows().find(
    (r) => within(r).getAllByRole("cell")[0].textContent?.trim() === name,
  );
  return Array.from(bodyRow?.querySelectorAll("td") ?? []);
}

const COLUMNS = {
  name: 0,
  pid: 1,
  cpu: 2,
  memory: 3,
  diskRead: 4,
  diskWrite: 5,
  gpu: 6,
} as const;

function cell(name: string, column: keyof typeof COLUMNS): HTMLTableCellElement {
  return rowCells(name)[COLUMNS[column]];
}

function cellsOf(name: string): string[] {
  return rowCells(name).map((element) => element.textContent?.trim() ?? "");
}

/** The table reads history through the store the page owns, so tests hand it one. */
function show(next: Tick, history = new MetricHistory()) {
  history.record(next);
  return render(<ProcessTable tick={next} history={history} />);
}

function nameColumn(): string[] {
  return bodyRows().map((r) => within(r).getAllByRole("cell")[0].textContent?.trim() ?? "");
}

function ariaSort(header: string): string | null {
  const th = screen.getByRole("button", { name: header }).closest("th");
  return th?.getAttribute("aria-sort") ?? null;
}

function filterInput(): HTMLElement {
  return screen.getByRole("searchbox", { name: "Filter by name or PID" });
}

describe("ProcessTable", () => {
  it("gives every process in the tick a row", () => {
    show(tick(rows));
    expect(bodyRows()).toHaveLength(3);
    expect(screen.getByText("3 processes")).toBeDefined();
  });

  it("prints a measured row as name, PID, CPU, memory, disk and GPU", () => {
    show(tick(rows));
    expect(cellsOf("notepad.exe")).toEqual([
      "notepad.exe",
      "4712",
      "12.5%",
      "20.0 MB",
      "1.0 KB/s",
      "—",
      "3.0%",
    ]);
  });

  it("shows an em dash, never a zero, for what Windows would not give", () => {
    show(tick(rows));
    expect(cellsOf("svchost.exe")).toEqual([
      "svchost.exe",
      "4",
      "—",
      "0 B",
      "—",
      "—",
      "—",
    ]);
  });

  it("opens on the busiest process so the heaviest user is on top", () => {
    show(tick(rows));
    expect(nameColumn()).toEqual(["Chrome.exe", "notepad.exe", "svchost.exe"]);
    expect(ariaSort("CPU")).toBe("descending");
    expect(ariaSort("Memory")).toBe("none");
  });

  it("flips a column when its header button is activated again", () => {
    show(tick(rows));
    const cpu = screen.getByRole("button", { name: "CPU" });
    fireEvent.click(cpu);
    expect(ariaSort("CPU")).toBe("ascending");
    fireEvent.click(cpu);
    expect(ariaSort("CPU")).toBe("descending");
  });

  it("moves the sort to the column the user just chose", () => {
    show(tick(rows));
    fireEvent.click(screen.getByRole("button", { name: "Memory" }));
    expect(ariaSort("Memory")).toBe("ascending");
    expect(ariaSort("CPU")).toBe("none");
  });

  it("sorts from the keyboard as well as the mouse", () => {
    show(tick(rows));
    const cpu = screen.getByRole("button", { name: "CPU" });
    cpu.focus();
    expect(document.activeElement).toBe(cpu);
    fireEvent.click(document.activeElement as HTMLElement);
    expect(ariaSort("CPU")).toBe("ascending");
  });

  it("filters by name without caring about case", () => {
    show(tick(rows));
    fireEvent.change(filterInput(), { target: { value: "NOTEPAD" } });
    expect(nameColumn()).toEqual(["notepad.exe"]);
    expect(screen.getByText("1 of 3 processes")).toBeDefined();
  });

  it("filters by PID", () => {
    show(tick(rows));
    fireEvent.change(filterInput(), { target: { value: "4712" } });
    expect(nameColumn()).toEqual(["notepad.exe"]);
  });

  it("says when nothing matches instead of showing a silent table", () => {
    show(tick(rows));
    fireEvent.change(filterInput(), { target: { value: "qqq" } });
    expect(screen.queryAllByRole("table")).toHaveLength(0);
    expect(screen.getByRole("status").textContent).toContain("qqq");

    fireEvent.click(screen.getByRole("button", { name: "Clear filter" }));
    expect(nameColumn()).toHaveLength(3);
    expect((filterInput() as HTMLInputElement).value).toBe("");
  });

  it("counts the rows that have no CPU or disk figure", () => {
    show(tick(rows));
    expect(
      screen.getByText("1 of 3 processes report no CPU or disk figure."),
    ).toBeDefined();
  });

  it("says nothing about unmeasured rows when every row was measured", () => {
    const measured = [row({ pid: 1, name: "a.exe", cpuPercent: 1 })];
    show(tick(measured));
    expect(screen.getByText("1 process")).toBeDefined();
    expect(screen.queryByText(/no CPU or disk figure/)).toBeNull();
  });

  it("marks the GPU column with the tick that actually read it", () => {
    show(tick(rows, { sequence: 15, gpu: sampling, gpuSequence: 12 }));
    expect(
      screen.getByRole("button", { name: /GPU.*tick 12/ }),
    ).toBeDefined();
  });

  it("leaves the GPU header plain on the tick that sampled it", () => {
    show(tick(rows, { sequence: 12, gpu: sampling, gpuSequence: 12 }));
    expect(screen.getByRole("button", { name: "GPU" })).toBeDefined();
  });

  it("explains a GPU figure that has not been measured yet", () => {
    show(tick(rows));
    expect(screen.getByText(/GPU has no reading yet/)).toBeDefined();
  });

  it("passes the reason through when this machine has no GPU counter", () => {
    show(
      tick(rows, { gpu: { kind: "unavailable", reason: "no GPU Engine performance object" } }),
    );
    expect(
      screen.getByText(/no GPU Engine performance object/),
    ).toBeDefined();
  });

  it("keeps a recycled PID as two processes, not one", () => {
    const samePid = [
      row({ pid: 4712, name: "old.exe", startTime: 100, cpuPercent: 1 }),
      row({ pid: 4712, name: "new.exe", startTime: 200, cpuPercent: 2 }),
    ];
    show(tick(samePid));
    expect(nameColumn()).toEqual(["new.exe", "old.exe"]);
  });

  it("draws one line per row, on the column the table is sorted by", () => {
    const history = new MetricHistory();
    history.record(
      tick([row({ pid: 7, name: "svc.exe", cpuPercent: 5, memoryBytes: 1024, gpuPercent: 2 })]),
    );
    const latest = tick(
      [row({ pid: 7, name: "svc.exe", cpuPercent: 45, memoryBytes: 2048, gpuPercent: 6 })],
      { sequence: 2 },
    );
    history.record(latest);
    const { container } = render(<ProcessTable tick={latest} history={history} />);

    expect(cell("svc.exe", "cpu").textContent).toBe("45.0%");
    expect(cell("svc.exe", "cpu").querySelector("svg")).not.toBeNull();
    expect(cell("svc.exe", "memory").querySelector("svg")).toBeNull();
    expect(cell("svc.exe", "gpu").querySelector("svg")).toBeNull();
    // History is kept for all five metrics so sorting can move the line, but
    // only one line per row is drawn — that is what keeps a 300-row tick
    // affordable, and the number is what one test per row can check.
    expect(container.querySelectorAll(".process-table__spark")).toHaveLength(1);
  });

  it("moves the line when the user sorts by another column", () => {
    const history = new MetricHistory();
    history.record(tick([row({ pid: 7, name: "svc.exe", cpuPercent: 5, memoryBytes: 1024 })]));
    const latest = tick([row({ pid: 7, name: "svc.exe", cpuPercent: 45, memoryBytes: 2048 })], {
      sequence: 2,
    });
    history.record(latest);
    render(<ProcessTable tick={latest} history={history} />);

    fireEvent.click(screen.getByRole("button", { name: "Memory" }));

    expect(cell("svc.exe", "memory").querySelector("svg")).not.toBeNull();
    expect(cell("svc.exe", "cpu").querySelector("svg")).toBeNull();
  });

  it("hides the line from assistive tech because the number is the reading", () => {
    const history = new MetricHistory();
    history.record(tick([row({ pid: 7, name: "svc.exe", cpuPercent: 5 })]));
    const latest = tick([row({ pid: 7, name: "svc.exe", cpuPercent: 45 })], { sequence: 2 });
    history.record(latest);
    render(<ProcessTable tick={latest} history={history} />);

    const svg = cell("svc.exe", "cpu").querySelector("svg");
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
    expect(svg?.getAttribute("focusable")).toBe("false");
  });

  it("prints the number alone while a row has nothing to draw yet", () => {
    show(tick([row({ pid: 7, name: "svc.exe", cpuPercent: 5 })]));
    expect(cell("svc.exe", "cpu").querySelector("svg")).toBeNull();
    expect(cell("svc.exe", "cpu").textContent).toBe("5.0%");
  });

  it("breaks the line where the machine gave no sample", () => {
    const history = new MetricHistory();
    const samples = [10, 20, null, 30, 40];
    let latest = tick([row({ pid: 7, name: "svc.exe", cpuPercent: samples[0] })]);

    samples.forEach((cpuPercent, index) => {
      latest = tick(
        [row({ pid: 7, name: "svc.exe", cpuPercent })],
        { sequence: index + 1 },
      );
      history.record(latest);
    });
    render(<ProcessTable tick={latest} history={history} />);

    expect(cell("svc.exe", "cpu").querySelectorAll("path")).toHaveLength(2);
  });

  it("has no accessibility violations", async () => {
    // Two ticks, so the lines are actually on screen when axe looks.
    const history = new MetricHistory();
    history.record(tick(rows));
    const { container } = show(
      tick(rows, { sequence: 15, gpu: sampling, gpuSequence: 12 }),
      history,
    );
    const results = await axe.run(container);
    expect(
      results.violations.map(
        (v) => `${v.id}: ${v.help} — ${v.nodes.map((n) => n.html).join(" | ")}`,
      ),
    ).toEqual([]);
  });
});
