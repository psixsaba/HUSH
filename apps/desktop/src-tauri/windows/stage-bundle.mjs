import { cpSync, copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const tauriDir = resolve(here, "..");
const root = resolve(tauriDir, "../../..");
const resources = join(tauriDir, "resources");
const release = join(root, "target", "release");

mkdirSync(join(resources, "driver"), { recursive: true });
copyFileSync(join(here, "install-driver.ps1"), join(resources, "install-driver.ps1"));

function copyIf(src, dest) {
  if (!existsSync(src)) {
    return false;
  }
  mkdirSync(dirname(dest), { recursive: true });
  copyFileSync(src, dest);
  console.log("staged", dest);
  return true;
}

copyIf(join(release, "hush.exe"), join(resources, "hush.exe"));
copyIf(join(release, "hush"), join(resources, "hush"));

function walkDlls(dir) {
  if (!existsSync(dir)) {
    return;
  }
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    let info;
    try {
      info = statSync(path);
    } catch {
      continue;
    }
    if (info.isDirectory()) {
      if (name === "incremental" || name === ".fingerprint" || name === "build") {
        continue;
      }
      walkDlls(path);
      continue;
    }
    if (name.toLowerCase().startsWith("onnxruntime") && name.toLowerCase().endsWith(".dll")) {
      copyIf(path, join(resources, name));
    }
  }
}

walkDlls(release);

const driverDir = process.env.HUSH_DRIVER_DIR || join(root, "dist", "driver");
if (existsSync(join(driverDir, "VirtualAudioDriver.inf")) || existsSync(join(driverDir, "virtualaudiodriver.inf"))) {
  cpSync(driverDir, join(resources, "driver"), { recursive: true });
  console.log("staged driver", driverDir);
} else {
  console.log("no driver package at", driverDir);
}
