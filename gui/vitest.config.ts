import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config.ts";

// The app's Vite config plus a test setup that pins the clock to 24 hours, so wording tests
// don't depend on the machine's locale.
export default mergeConfig(
  viteConfig,
  defineConfig({ test: { setupFiles: ["./src/lib/testSetup.ts"] } }),
);
