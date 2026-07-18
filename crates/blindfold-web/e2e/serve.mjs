// A static file server for the built `dist/`, used only by the Playwright
// webServer. Committed rather than pulled in as a dependency: the test needs to
// serve one directory with the right `application/wasm` type (browsers refuse to
// stream-compile wasm served as octet-stream), and nothing more.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../dist/", import.meta.url));
// The port comes from playwright.config.js on the command line — that config is
// the single source of truth for it. Required, not defaulted: this server exists
// only for the Playwright webServer, which always passes it.
const PORT = Number(process.argv[2]);
if (!Number.isInteger(PORT) || PORT <= 0) {
  console.error("usage: node serve.mjs <port>");
  process.exit(1);
}

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json; charset=utf-8",
  ".ico": "image/x-icon",
};

const server = createServer(async (req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
  if (path === "/") path = "/index.html";
  const file = normalize(join(ROOT, path));
  // Refuse anything that escapes the served directory.
  if (file !== ROOT.slice(0, -1) && !file.startsWith(ROOT.endsWith(sep) ? ROOT : ROOT + sep)) {
    res.writeHead(403).end("forbidden");
    return;
  }
  try {
    const body = await readFile(file);
    res.writeHead(200, { "content-type": TYPES[extname(file)] || "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end("not found");
  }
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`serving ${ROOT} at http://127.0.0.1:${PORT}`);
});
