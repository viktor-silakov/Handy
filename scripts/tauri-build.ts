import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

function removeMatchingDmgs(dir: string) {
  if (!existsSync(dir)) {
    return;
  }

  for (const entry of readdirSync(dir)) {
    if (!entry.endsWith(".dmg")) {
      continue;
    }

    rmSync(join(dir, entry), { force: true });
  }
}

function getArchSuffix() {
  switch (process.arch) {
    case "arm64":
      return "aarch64";
    case "x64":
      return "x64";
    default:
      throw new Error(`Unsupported architecture for DMG packaging: ${process.arch}`);
  }
}

function createDmg(root: string) {
  if (process.platform !== "darwin") {
    return;
  }

  const tauriConfig = JSON.parse(
    readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
  ) as { productName: string; version: string };

  const productName = tauriConfig.productName;
  const version = tauriConfig.version;
  const arch = getArchSuffix();
  const macosBundleDir = join(root, "src-tauri/target/release/bundle/macos");
  const dmgBundleDir = join(root, "src-tauri/target/release/bundle/dmg");
  const appPath = join(macosBundleDir, `${productName}.app`);
  const dmgPath = join(dmgBundleDir, `${productName}_${version}_${arch}.dmg`);

  const stageDir = mkdtempSync(join(tmpdir(), `${productName}-dmg-stage-`));
  cpSync(appPath, join(stageDir, `${productName}.app`), { recursive: true });
  symlinkSync("/Applications", join(stageDir, "Applications"));

  const result = spawnSync(
    "hdiutil",
    ["create", "-volname", productName, "-srcfolder", stageDir, "-ov", "-format", "UDZO", dmgPath],
    { stdio: "inherit" },
  );

  rmSync(stageDir, { recursive: true, force: true });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const root = process.cwd();
removeMatchingDmgs(join(root, "src-tauri/target/release/bundle/macos"));
removeMatchingDmgs(join(root, "src-tauri/target/release/bundle/dmg"));

const forwardedArgs = process.argv.slice(2);
const hasExplicitBundles = forwardedArgs.some(
  (arg, index) =>
    arg === "--bundles" ||
    arg === "-b" ||
    forwardedArgs[index - 1] === "--bundles" ||
    forwardedArgs[index - 1] === "-b",
);

const shouldRunLocalMacBuild = forwardedArgs[0] === "build" && !hasExplicitBundles;
const tauriArgs = shouldRunLocalMacBuild
  ? [
      "build",
      "--bundles",
      "app",
      "--config",
      '{"bundle":{"createUpdaterArtifacts":false}}',
      ...forwardedArgs.slice(1),
    ]
  : forwardedArgs;

const result = spawnSync("tauri", tauriArgs, {
  stdio: "inherit",
});

if (result.error) {
  throw result.error;
}

if (result.status === 0 && shouldRunLocalMacBuild) {
  createDmg(root);
}

process.exit(result.status ?? 1);
