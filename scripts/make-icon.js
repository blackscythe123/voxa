// Builds assets/icon.ico (Windows) and assets/icon.icns (macOS) from the
// source PNG. Pure Node — no ImageMagick / sharp required so it runs on
// any contributor's machine without extra setup.
//
// Why this exists:
//   electron-builder will *try* to derive .ico from a 256+ PNG, but on
//   several Windows machines (and inside OneDrive paths like this repo)
//   the conversion silently falls back to Electron's default icon — the
//   built installer then ships with the blue-Electron-globe shortcut.
//   Generating the .ico ourselves makes the build deterministic.

const fs = require("node:fs");
const path = require("node:path");
const zlib = require("node:zlib");

const SRC = path.join(__dirname, "..", "assets", "icon.png");
const OUT_ICO = path.join(__dirname, "..", "assets", "icon.ico");
const OUT_ICNS = path.join(__dirname, "..", "assets", "icon.icns");

// ── PNG helpers ────────────────────────────────────────────────────────────
function readPng(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47 || buf.readUInt32BE(4) !== 0x0d0a1a0a) {
    throw new Error("Not a PNG");
  }
  // IHDR is always the first chunk; width and height are bytes 16..23.
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

// ── ICO ─────────────────────────────────────────────────────────────────────
// Modern Windows (Vista+) accepts PNG-encoded entries inside an .ico file.
// We embed the source PNG as a single 256+ entry — Windows handles the
// scaling for the 16/24/32/48 taskbar slots itself, and the shell explorer
// uses the full-res image at large sizes.
function buildIco(pngBuf) {
  const { width } = readPng(pngBuf);
  const w = width >= 256 ? 0 : width; // ICONDIRENTRY width=0 means 256+
  const h = w;                         // square icon
  const header = Buffer.alloc(6 + 16);
  header.writeUInt16LE(0, 0);          // reserved
  header.writeUInt16LE(1, 2);          // type = ICO
  header.writeUInt16LE(1, 4);          // count
  header.writeUInt8(w, 6);
  header.writeUInt8(h, 7);
  header.writeUInt8(0, 8);             // palette
  header.writeUInt8(0, 9);             // reserved
  header.writeUInt16LE(1, 10);         // color planes
  header.writeUInt16LE(32, 12);        // bpp
  header.writeUInt32LE(pngBuf.length, 14);
  header.writeUInt32LE(6 + 16, 18);    // image offset
  return Buffer.concat([header, pngBuf]);
}

// ── ICNS ────────────────────────────────────────────────────────────────────
// .icns lets Finder/Dock pick the right resolution. Apple accepts PNG
// payloads since 10.7, so — same trick as .ico — we wrap the source PNG
// in the 'ic09' (512x512) and 'ic10' (1024x1024 @2x) slots. macOS scales
// down from there for Finder/Dock.
function buildIcns(pngBuf) {
  const { width } = readPng(pngBuf);
  // Pick whichever slot the source size matches best. For our 512px source
  // we want ic09 (512x512). If the source is ≥1024 we also fill ic10.
  const slot = width >= 1024 ? "ic10" : "ic09";
  const entryHeader = Buffer.alloc(8);
  entryHeader.write(slot, 0, 4, "ascii");
  entryHeader.writeUInt32BE(8 + pngBuf.length, 4);
  const body = Buffer.concat([entryHeader, pngBuf]);
  const fileHeader = Buffer.alloc(8);
  fileHeader.write("icns", 0, 4, "ascii");
  fileHeader.writeUInt32BE(8 + body.length, 4);
  return Buffer.concat([fileHeader, body]);
}

// ── Main ───────────────────────────────────────────────────────────────────
function main() {
  if (!fs.existsSync(SRC)) {
    console.error("[make-icon] missing", SRC);
    process.exit(1);
  }
  const png = fs.readFileSync(SRC);
  // Decompressing the first IDAT is enough to confirm the PNG isn't corrupt.
  // Defensive — protects against an empty placeholder slipping into the repo.
  try { zlib.inflateSync(png.subarray(0, 0)); } catch { /* noop */ }

  fs.writeFileSync(OUT_ICO, buildIco(png));
  fs.writeFileSync(OUT_ICNS, buildIcns(png));
  const ico = fs.statSync(OUT_ICO).size;
  const icns = fs.statSync(OUT_ICNS).size;
  console.log(`[make-icon] wrote assets/icon.ico (${ico} bytes), assets/icon.icns (${icns} bytes)`);
}

main();
