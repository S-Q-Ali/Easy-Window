import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    // Testing Library registers its own teardown through the global `afterEach`.
    globals: true,
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
