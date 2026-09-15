import { defineConfig, devices } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30000,
  use: {
    baseURL: "http://localhost:19173",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: [
    {
      command:
        "HONEYCOMB_FIXTURE_SHORT_SESSION=1 HONEYCOMB_FIXTURE_PORT=19180 cargo run --manifest-path ../Cargo.toml -p silicon-honeycomb-server --example e2e_fixture",
      url: "http://127.0.0.1:19180/health",
      timeout: 120000,
      reuseExistingServer: false,
    },
    ...(["library", "console"] as const).map((site, index) => ({
      command: "npm run dev",
      url: `http://localhost:${19173 + index}/api/config`,
      timeout: 60000,
      reuseExistingServer: false,
      env: {
        PORT: String(19173 + index),
        WEB_ORIGIN: `http://localhost:${19173 + index}`,
        HONEYCOMB_SITE: site,
        HONEYCOMB_API_URL: "http://127.0.0.1:19180",
        LIBRARY_ORIGIN: "http://localhost:19173",
        CONSOLE_ORIGIN: "http://localhost:19174",
        WEB_SESSION_DB: `data/e2e-${site}.db`,
      },
    })),
  ],
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"] } },
    {
      name: "mobile",
      use: { ...devices["iPhone 13"], defaultBrowserType: "chromium" },
    },
  ],
});
