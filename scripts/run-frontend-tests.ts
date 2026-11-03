// Runs every standalone frontend check (src/**/*.test.ts), each with bun, and
// fails if any does. `bun run test:frontend`.
import { Glob } from "bun";

const files = [...new Glob("src/**/*.test.ts").scanSync(".")].sort();
let failed = 0;
for (const file of files) {
  const run = Bun.spawnSync(["bun", file], { stdout: "pipe", stderr: "pipe" });
  const out = (run.stdout.toString() + run.stderr.toString()).trim();
  if (run.exitCode === 0) {
    console.log(`ok    ${file}`);
  } else {
    failed++;
    console.log(`FAIL  ${file}\n${out}`);
  }
}
console.log(
  `${files.length - failed}/${files.length} frontend test files passed`,
);
process.exit(failed ? 1 : 0);
