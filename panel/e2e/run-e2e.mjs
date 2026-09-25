// The e2e suite's ONE entry point, so the specs that write the daemon's real
// config can only run on purpose. See e2e/write-guard.ts for why this is a gate
// rather than a reminder.
//
//   npm run test:e2e      -> sets the flag, runs everything
//   npx playwright test   -> does NOT set it; registry.spec.ts skips, with a reason
import { spawn } from "node:child_process";

process.env.RUAGENT_E2E_ALLOW_WRITES = "1";

const args = process.argv.slice(2);
const child = spawn("npx", ["playwright", "test", ...args], {
  stdio: "inherit",
  shell: true,
  env: process.env,
});
child.on("exit", (code) => process.exit(code ?? 1));
