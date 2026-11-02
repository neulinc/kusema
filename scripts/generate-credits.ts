// Builds the About page's "Credits and licenses" from what Kusema really uses:
// Rust crates from `cargo metadata`, JS packages from package.json and
// node_modules, speech models from the catalog. Each package's own license files
// go into one notices file, identical texts together.
//
//   bun run credits          regenerate
//   bun run credits --check  fail if the committed files are out of date
import { spawnSync } from "node:child_process";
import {
  existsSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { catalog } from "@handy-computer/models";
import { isOfferedByKusema } from "../src/lib/models/removed";

const ROOT = join(import.meta.dir, "..");
const LIST_FILE = join(ROOT, "src/content/credits.json");
const NOTICES_FILE = join(ROOT, "src/content/third-party-notices.txt");
const NVIDIA_NOTICE =
  "Licensed by NVIDIA Corporation under the NVIDIA Open Model License";
const NVIDIA_LICENSE_URL =
  "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/";

interface Component {
  name: string;
  version: string;
  license: string;
  url: string;
}

interface Model {
  name: string;
  license: string;
  url: string;
  notice?: string;
}

const LICENSE_NAMES: Record<string, string> = {
  "cc-by-4.0": "CC BY 4.0",
  "cc-by-sa-4.0": "CC BY-SA 4.0",
  "apache-2.0": "Apache 2.0",
  mit: "MIT",
};

// The catalog's label is out of date or just says "other" for these. Read from
// their model cards; see "Licenses to re-check" in docs/kusema-build-plan.md.
const LICENSE_CORRECTIONS: Record<string, string> = {
  "parakeet-unified-en-0.6b": "NVIDIA Open Model License",
  "multitalker-parakeet-streaming-0.6b-v1": "NVIDIA Open Model License",
  "nemotron-speech-streaming-en-0.6b": "NVIDIA Open Model License",
  "nemotron-3.5-asr-streaming-0.6b": "OpenMDW",
};

// Not in the catalog: Kusema's own voices, the voice detector and the old files.
const OTHER_MODELS: Model[] = [
  {
    name: "Kokoro-82M voices",
    license: "Apache 2.0",
    url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX",
  },
  {
    name: "misaki phonemizer data",
    license: "Apache 2.0",
    url: "https://github.com/hexgrad/misaki",
  },
  {
    name: "Silero VAD",
    license: "MIT",
    url: "https://github.com/snakers4/silero-vad",
  },
  {
    name: "Whisper (GGML files)",
    license: "MIT",
    url: "https://github.com/openai/whisper",
  },
  {
    name: "Breeze ASR 25 (GGML file)",
    license: "Apache 2.0",
    url: "https://huggingface.co/MediaTek-Research/Breeze-ASR-25",
  },
];

// Downloaded at run time, so in no package manifest.
const OTHER_COMPONENTS: Component[] = [
  {
    name: "ONNX Runtime",
    version: "",
    license: "MIT",
    url: "https://github.com/microsoft/onnxruntime",
  },
];

/** Identical license texts, each with the packages it came from. */
const texts = new Map<string, Set<string>>();
const missingTexts: string[] = [];

const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright|authors$)/i;

const collectLicenseFiles = (dir: string, label: string, license: string) => {
  let found = false;
  for (const name of readdirSync(dir).sort()) {
    const path = join(dir, name);
    if (!LICENSE_FILE.test(name) || !statSync(path).isFile()) continue;
    const text = readFileSync(path, "utf8").replace(/\r\n/g, "\n").trim();
    if (!text) continue;
    if (!texts.has(text)) texts.set(text, new Set());
    texts.get(text)!.add(label);
    found = true;
  }
  if (!found) missingTexts.push(`${label}: ${license}`);
};

const rustComponents = (): Component[] => {
  const run = spawnSync(
    "cargo",
    [
      "metadata",
      "--format-version",
      "1",
      "--locked",
      "--filter-platform",
      "x86_64-pc-windows-msvc",
    ],
    { cwd: join(ROOT, "src-tauri"), encoding: "utf8", maxBuffer: 1 << 30 },
  );
  if (run.status !== 0) throw new Error(`cargo metadata failed: ${run.stderr}`);
  const metadata = JSON.parse(run.stdout);
  const nodes = new Map<string, any>(
    metadata.resolve.nodes.map((node: any) => [node.id, node]),
  );
  const packages = new Map<string, any>(
    metadata.packages.map((p: any) => [p.id, p]),
  );
  const root = metadata.resolve.root ?? metadata.workspace_members[0];

  // What ships: normal dependencies only, not build or dev ones.
  const seen = new Set<string>();
  const walk = (id: string) => {
    for (const dep of nodes.get(id)?.deps ?? []) {
      const ships = dep.dep_kinds.some((kind: any) => kind.kind === null);
      if (ships && !seen.has(dep.pkg)) {
        seen.add(dep.pkg);
        walk(dep.pkg);
      }
    }
  };
  walk(root);

  return [...seen].map((id) => {
    const p = packages.get(id);
    const license = p.license ?? "See the package";
    collectLicenseFiles(
      dirname(p.manifest_path),
      `${p.name} ${p.version}`,
      license,
    );
    return {
      name: p.name,
      version: p.version,
      license,
      url: p.repository ?? p.homepage ?? `https://crates.io/crates/${p.name}`,
    };
  });
};

const jsLicense = (pkg: any): string => {
  if (typeof pkg.license === "string") return pkg.license;
  if (pkg.license?.type) return pkg.license.type;
  if (Array.isArray(pkg.licenses))
    return pkg.licenses.map((l: any) => l.type).join(" OR ");
  return "See the package";
};

const jsUrl = (pkg: any): string => {
  const repository =
    typeof pkg.repository === "string" ? pkg.repository : pkg.repository?.url;
  if (!repository)
    return pkg.homepage ?? `https://www.npmjs.com/package/${pkg.name}`;
  return repository
    .replace(/^git\+/, "")
    .replace(/\.git$/, "")
    .replace(/^git:\/\//, "https://")
    .replace(/^ssh:\/\/git@/, "https://")
    .replace(/^github:/, "https://github.com/");
};

const jsComponents = (): Component[] => {
  const found = new Map<string, Component>();
  const locate = (name: string, from: string): string | null => {
    let dir = from;
    for (;;) {
      const candidate = join(dir, "node_modules", name);
      if (existsSync(join(candidate, "package.json"))) return candidate;
      if (dir === ROOT) return null;
      dir = dirname(dir);
    }
  };
  const walk = (name: string, from: string) => {
    const dir = locate(name, from);
    if (!dir) return;
    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    const key = `${pkg.name}@${pkg.version}`;
    if (found.has(key)) return;
    const license = jsLicense(pkg);
    found.set(key, {
      name: pkg.name,
      version: pkg.version,
      license,
      url: jsUrl(pkg),
    });
    collectLicenseFiles(dir, `${pkg.name} ${pkg.version}`, license);
    for (const dep of Object.keys(pkg.dependencies ?? {})) walk(dep, dir);
  };
  const app = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
  for (const dep of Object.keys(app.dependencies ?? {})) walk(dep, ROOT);
  return [...found.values()];
};

const speechModels = (): Model[] => {
  const unknown: string[] = [];
  // What the app can download: speech models, and the language detector they use.
  const offered = catalog.models.filter(
    (m: any) =>
      isOfferedByKusema(m) &&
      (m.role === "asr" || m.id === catalog.languageDetectionModel?.id),
  );
  const models: Model[] = offered.map((m: any) => {
    const label = String(m.license?.id ?? m.license).toLowerCase();
    const license = LICENSE_CORRECTIONS[m.id] ?? LICENSE_NAMES[label];
    if (!license) unknown.push(`${m.id} (${label})`);
    return {
      name: m.name,
      license: license ?? label,
      url: `https://huggingface.co/${m.repository}`,
      ...(license === "NVIDIA Open Model License"
        ? { notice: NVIDIA_NOTICE }
        : {}),
    };
  });
  // The label can't be trusted to mean anything by itself: someone has to read it.
  if (unknown.length)
    throw new Error(`Review these models' licenses:\n${unknown.join("\n")}`);
  return models;
};

const byName = <T extends { name: string }>(items: T[]): T[] =>
  [...items].sort((a, b) => a.name.localeCompare(b.name, "en"));

const rust = byName(rustComponents());
const js = byName(jsComponents());
const models = byName([...speechModels(), ...OTHER_MODELS]);
const others = byName(OTHER_COMPONENTS);

const list = `${JSON.stringify(
  {
    nvidiaNotice: NVIDIA_NOTICE,
    nvidiaLicenseUrl: NVIDIA_LICENSE_URL,
    models,
    rust,
    js,
    others,
  },
  null,
  1,
)}\n`;

const RULE = "=".repeat(78);
const handyLicense = readFileSync(join(ROOT, "LICENSE"), "utf8")
  .replace(/\r\n/g, "\n")
  .trim();
const nvidiaLicense = readFileSync(
  join(ROOT, "src/content/licenses/nvidia-open-model-license.txt"),
  "utf8",
)
  .replace(/\r\n/g, "\n")
  .trim();
const notices = [
  "Kusema third-party notices. Generated by scripts/generate-credits.ts.",
  "",
  RULE,
  "Handy",
  RULE,
  "Kusema is built on Handy by CJ Pais, used under the MIT License.",
  "https://github.com/cjpais/Handy",
  "",
  handyLicense,
  "",
  RULE,
  "NVIDIA models",
  RULE,
  NVIDIA_NOTICE,
  "",
  "Applies to:",
  ...models.filter((m) => m.notice).map((m) => `  ${m.name}`),
  "",
  nvidiaLicense,
  "",
  RULE,
  "Speech and voice models",
  RULE,
  ...models.map((m) => `${m.name}: ${m.license}\n  ${m.url}`),
  "",
  RULE,
  "Other components",
  RULE,
  ...others.map((c) => `${c.name}: ${c.license}\n  ${c.url}`),
  "",
  ...[...texts.entries()]
    .sort(([, a], [, b]) =>
      [...a].sort()[0].localeCompare([...b].sort()[0], "en"),
    )
    .flatMap(([text, labels]) => [
      RULE,
      ...[...labels].sort((a, b) => a.localeCompare(b, "en")),
      RULE,
      text,
      "",
    ]),
  RULE,
  "Packages with no license file of their own, under the license they declare",
  RULE,
  ...missingTexts.sort((a, b) => a.localeCompare(b, "en")),
  "",
].join("\n");

if (process.argv.includes("--check")) {
  const same = (file: string, text: string) =>
    existsSync(file) &&
    readFileSync(file, "utf8").replace(/\r\n/g, "\n") === text;
  if (!same(LIST_FILE, list) || !same(NOTICES_FILE, notices)) {
    console.error("Credits are out of date. Run: bun run credits");
    process.exit(1);
  }
  console.log("Credits are up to date.");
} else {
  writeFileSync(LIST_FILE, list);
  writeFileSync(NOTICES_FILE, notices);
}
console.log(
  `${rust.length} Rust crates, ${js.length} JS packages, ${models.length} models, ` +
    `${texts.size} license texts, ${missingTexts.length} with no license file, ` +
    `${Math.round(notices.length / 1024)} KB`,
);
