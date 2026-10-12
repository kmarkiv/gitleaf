import { build } from 'esbuild';
import { cp, mkdir, rm, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

const directory = 'static/assets';
await rm(directory, { recursive: true, force: true });
await mkdir(directory, { recursive: true });
const result = await build({
  entryPoints: { app: 'frontend/app.js' }, outdir: directory,
  bundle: true, splitting: true, format: 'esm', minify: true,
  target: ['chrome123', 'firefox128', 'safari17.4'], metafile: true,
  entryNames: '[name]', chunkNames: 'chunks/[name]-[hash]',
  assetNames: 'fonts/[name]-[hash]',
  loader: { '.woff2': 'file', '.woff': 'file', '.ttf': 'file' },
  legalComments: 'linked',
});
await cp('node_modules/pdfjs-dist/build/pdf.worker.min.mjs', `${directory}/pdf.worker.min.mjs`);
for (const folder of ['cmaps', 'standard_fonts', 'wasm', 'iccs']) {
  await cp(`node_modules/pdfjs-dist/${folder}`, `${directory}/${folder}`, { recursive: true });
}
const roots = new Set();
for (const input of Object.keys(result.metafile.inputs)) {
  const match = input.match(/^node_modules\/(?:@[^/]+\/)?[^/]+/);
  if (match) roots.add(match[0]);
}
let notices = 'GitLeaf browser dependency notices\n\n';
for (const root of [...roots].sort()) {
  const pkg = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
  notices += `${pkg.name} ${pkg.version} (${pkg.license})\n`;
  for (const name of ['LICENSE', 'LICENSE.txt', 'LICENSE.md']) {
    try { notices += `${await readFile(path.join(root, name), 'utf8')}\n\n`; break; } catch {}
  }
}
await writeFile(`${directory}/THIRD_PARTY_NOTICES.txt`, notices);
console.log(`Bundled ${roots.size} browser dependencies with PDF worker, fonts and decoder assets.`);
