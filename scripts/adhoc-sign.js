// electron-builder afterPack hook: ad-hoc sign the final macOS app with the
// entitlements. Without a signature (and the audio-input entitlement) macOS
// can't remember the Microphone/Accessibility grants, so it re-prompts forever.
// shortcut: ad-hoc identity changes every build, so permissions reset on update; use a Developer ID to fix.
const path = require("node:path");
const { execFileSync } = require("node:child_process");

exports.default = async function adhocSign(context) {
  if (context.electronPlatformName !== "darwin") return;
  // Universal builds pack x64/arm64 temp apps first; signing those breaks the merge.
  if (/-temp$/.test(context.appOutDir)) return;

  const app = path.join(context.appOutDir, `${context.packager.appInfo.productFilename}.app`);
  const entitlements = path.join(context.packager.projectDir, "build", "entitlements.mac.plist");
  execFileSync(
    "codesign",
    ["--force", "--deep", "--sign", "-", "--options", "runtime", "--entitlements", entitlements, app],
    { stdio: "inherit" }
  );
  execFileSync("codesign", ["--verify", "--deep", "--strict", app], { stdio: "inherit" });
};
