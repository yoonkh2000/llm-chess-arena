import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  outputDir: "target/playwright-results",
  reporter: "line",
  timeout: 45_000,
  retries: 1,
  use: {
    baseURL: "http://127.0.0.1:8780/llm-chess-arena/",
    trace: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "NO_COLOR=true trunk serve --public-url /llm-chess-arena/ --port 8780",
    url: "http://127.0.0.1:8780/llm-chess-arena/",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
