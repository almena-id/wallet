// Writes a release's version into src-tauri/tauri.conf.json, in the shape
// each platform accepts. Run by the release workflow on its own checkout; the
// committed file keeps its development version.
//
//   node .github/scripts/stamp-version.mjs 2026.09.1
//
// The release is year.month.sequence, the month with its leading zero (the
// git tag is v2026.09.1). What each platform is given:
//
//   version       2026.9.1     SemVer refuses the leading zero; macOS and iOS
//                              read it as CFBundleShortVersionString and
//                              CFBundleVersion, Linux and NSIS as it is
//   wix.version   26.9.1       an MSI version's first field stops at 255
//   versionCode   2026009001   Android: year·10⁶ + month·10³ + sequence, which
//                              only grows while the sequence stays under 1000

import { readFileSync, writeFileSync } from "node:fs";

const release = process.argv[2] ?? "";
const match = /^(\d{4})\.(\d{2})\.(\d+)$/.exec(release);
if (!match) {
  console.error(`Not a year.month.sequence version: "${release}"`);
  process.exit(1);
}
const [year, month, sequence] = match.slice(1).map(Number);
if (sequence >= 1000) {
  console.error(`Sequence ${sequence} is past what Android's versionCode can hold in order.`);
  process.exit(1);
}

const path = new URL("../../src-tauri/tauri.conf.json", import.meta.url);
const conf = JSON.parse(readFileSync(path, "utf8"));

conf.version = `${year}.${month}.${sequence}`;
conf.bundle.windows = {
  ...conf.bundle.windows,
  wix: { ...conf.bundle.windows?.wix, version: `${year - 2000}.${month}.${sequence}` },
};
conf.bundle.android = {
  ...conf.bundle.android,
  versionCode: year * 1_000_000 + month * 1_000 + sequence,
};

writeFileSync(path, `${JSON.stringify(conf, null, 2)}\n`);
console.log(
  `version ${conf.version}, MSI ${conf.bundle.windows.wix.version}, versionCode ${conf.bundle.android.versionCode}`,
);
