import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = new URL("../", import.meta.url);
const files = ["index.html", "style.css", "settings.js", "site.js", "logo.png"];
const mime = ["text/html; charset=utf-8", "text/css; charset=utf-8", "text/javascript; charset=utf-8",
  "text/javascript; charset=utf-8", "image/png"];
const assets = {};

for (const [index, name] of files.entries()) {
  let data = await readFile(new URL("site/" + name, root));
  if (name === "settings.js") {
    data = Buffer.concat([data, Buffer.from("\nwindow.twiKeyApi = window.location.origin;\n")]);
  }
  assets["/" + name] = { type: mime[index], data: data.toString("base64") };
}
assets["/"] = assets["/index.html"];

await build({
  stdin: {
    contents: `import { makeWorker } from "./backend/index.mjs"; export default makeWorker(${JSON.stringify(assets)});`,
    resolveDir: fileURLToPath(root)
  },
  outfile: fileURLToPath(new URL("dist/server/index.js", root)),
  bundle: true,
  format: "esm",
  target: "es2022"
});

try {
  const manifest = await readFile(new URL(".openai/hosting.json", root));
  await mkdir(new URL("dist/.openai", root), { recursive: true });
  await writeFile(new URL("dist/.openai/hosting.json", root), manifest);
} catch (error) {
  if (error.code !== "ENOENT") throw error;
}
