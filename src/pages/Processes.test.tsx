import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SamplingState, Tick } from "../lib/metrics";
import { row, tick } from "../components/ProcessTable/fixtures";
import { Processes } from "./Processes";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));

type Listener = (event: { payload: Tick }) => void;

let listener: Listener | undefined;
let unlisten: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.clearAllMocks();
  listener = undefined;
  unlisten = vi.fn();
  mocks.listen.mockImplementation(
    async (_event: string, handler: Listener) => {
      listener = handler;
      return unlisten;
    },
  );
});

function invokeOnce(state: SamplingState | Error) {
  mocks.invoke.mockImplementation(() =>
    state instanceof Error
      ? Promise.reject(state)
      : Promise.resolve(state),
  );
}

describe("Processes page", () => {
  it("waits for the first tick instead of showing an empty table", async () => {
    invokeOnce({ state: "running", tick: null });
    render(<Processes />);

    expect((await screen.findByRole("status")).textContent).toMatch(/first tick/);
    expect(screen.queryAllByRole("table")).toHaveLength(0);
  });

  it("paints the tick the pull already handed back", async () => {
    invokeOnce({
      state: "running",
      tick: tick([row({ pid: 1, name: "notepad.exe", cpuPercent: 4 })]),
    });
    render(<Processes />);

    expect(await screen.findByText("notepad.exe")).toBeDefined();
  });

  it("keeps painting every tick the loop pushes", async () => {
    invokeOnce({
      state: "running",
      tick: tick([row({ pid: 1, name: "notepad.exe", cpuPercent: 4 })]),
    });
    render(<Processes />);
    await screen.findByText("notepad.exe");

    expect(mocks.listen).toHaveBeenCalledWith("metrics://tick", expect.any(Function));

    await act(() =>
      listener?.({
        payload: tick([row({ pid: 2, name: "code.exe", cpuPercent: 9 })], { sequence: 2 }),
      }),
    );

    expect(screen.getByText("code.exe")).toBeDefined();
    expect(screen.queryByText("notepad.exe")).toBeNull();
  });

  it("reports a sampler that could not start, with its reason", async () => {
    invokeOnce({ state: "failed", reason: "the performance-data helper refused" });
    render(<Processes />);

    expect(
      await screen.findByText(/the performance-data helper refused/),
    ).toBeDefined();
    expect(screen.queryAllByRole("table")).toHaveLength(0);
  });

  it("reports a monitor it could not reach at all", async () => {
    invokeOnce(new Error("Command latest_tick not found"));
    render(<Processes />);

    expect(await screen.findByText(/latest_tick not found/)).toBeDefined();
    expect(screen.queryAllByRole("table")).toHaveLength(0);
  });

  it("ignores a tick older than the one already on screen", async () => {
    invokeOnce({
      state: "running",
      tick: tick([row({ pid: 2, name: "code.exe", cpuPercent: 9 })], { sequence: 5 }),
    });
    render(<Processes />);
    await screen.findByText("code.exe");

    await act(() =>
      listener?.({
        payload: tick([row({ pid: 1, name: "notepad.exe", cpuPercent: 4 })], { sequence: 4 }),
      }),
    );

    expect(screen.getByText("code.exe")).toBeDefined();
    expect(screen.queryByText("notepad.exe")).toBeNull();
  });

  it("gives the table a line that grows from the ticks it has been shown", async () => {
    invokeOnce({
      state: "running",
      tick: tick([row({ pid: 1, name: "notepad.exe", cpuPercent: 4 })]),
    });
    const { container } = render(<Processes />);
    await screen.findByText("notepad.exe");

    // One sample is not a trend, so the pull alone draws no line.
    expect(container.querySelector("svg")).toBeNull();

    await act(() =>
      listener?.({
        payload: tick([row({ pid: 1, name: "notepad.exe", cpuPercent: 9 })], { sequence: 2 }),
      }),
    );

    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
    expect(svg?.querySelector("path")).not.toBeNull();
  });

  it("stops listening when the page leaves the screen", async () => {
    invokeOnce({
      state: "running",
      tick: tick([row({ pid: 1, name: "notepad.exe", cpuPercent: 4 })]),
    });
    const { unmount } = render(<Processes />);
    await screen.findByText("notepad.exe");

    unmount();
    expect(unlisten).toHaveBeenCalled();
  });
});
