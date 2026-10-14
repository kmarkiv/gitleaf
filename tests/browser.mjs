// End-to-end regression checks use isolated projects and a deterministic test compiler.
// They never read or change the user's papers or Git configuration.
import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { rename, mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';

const directory = await mkdtemp(path.join(tmpdir(), 'gitleaf-browser-'));
const root = path.join(directory, 'projects'), cache = path.join(directory, 'cache'), bin = path.join(directory, 'bin');
await mkdir(bin, { recursive: true });
function pdf(text) {
  const stream = `BT /F1 24 Tf 72 720 Td (${text}) Tj ET`;
  const objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>', '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>', '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>', `<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`];
  let output = '%PDF-1.4\n', offsets = [0];
  objects.forEach((object, i) => { offsets.push(Buffer.byteLength(output)); output += `${i + 1} 0 obj\n${object}\nendobj\n`; });
  const start = Buffer.byteLength(output);
  output += `xref\n0 ${offsets.length}\n0000000000 65535 f \n${offsets.slice(1).map(offset => String(offset).padStart(10, '0') + ' 00000 n ').join('\n')}\ntrailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${start}\n%%EOF\n`;
  return Buffer.from(output);
}
await writeFile(path.join(directory, 'sample.pdf'), pdf('GitLeaf offline preview'));
await writeFile(path.join(bin, 'tectonic'), `#!/usr/bin/env node
const fs = require('node:fs'), path = require('node:path'), zlib = require('node:zlib');
const args = process.argv.slice(2);
if (args.includes('--version')) { console.log('tectonic browser test fixture'); process.exit(0); }
const out = args[args.indexOf('--outdir')+1], main = args.at(-1), stem = path.basename(main, '.tex');
fs.mkdirSync(out, {recursive:true});
const source = fs.readFileSync(main,'utf8');
if (source.includes('undefinedcmd')) {
  fs.writeFileSync(path.join(out,stem+'.pdf'),'incomplete PDF from a failed compile');
  fs.writeFileSync(path.join(out,stem+'.log'),'! Undefined control sequence.\\nl.4 \\\\undefinedcmd\\n');
  process.exit(1);
}
fs.copyFileSync(${JSON.stringify(path.join(directory, 'sample.pdf'))},path.join(out,stem+'.pdf'));
fs.writeFileSync(path.join(out,stem+'.log'),'Compilation complete');
fs.writeFileSync(path.join(out,stem+'.synctex.gz'),zlib.gzipSync('SyncTeX Version:1\\nInput:1:./'+main+'\\nContent:\\n{1\\nx1,1:4736286,4736286\\n}1\\n'));
`, { mode: 0o755 });

const socket = createServer(); socket.listen(0, '127.0.0.1'); await once(socket, 'listening');
const port = socket.address().port; await new Promise(resolve => socket.close(resolve));
const binary = process.env.GITLEAF_BIN || path.resolve('target/debug/gitleaf');
const server = spawn(binary, ['--root', root, '--cache', cache, '--port', String(port), '--no-open'], {
  env: { ...process.env, GITLEAF_NO_UPDATE_CHECK: '1', PATH: `${bin}${path.delimiter}${process.env.PATH}`, GITLEAF_ENGINE: 'tectonic', GIT_AUTHOR_NAME: 'GitLeaf Test', GIT_AUTHOR_EMAIL: 'test@example.invalid', GIT_COMMITTER_NAME: 'GitLeaf Test', GIT_COMMITTER_EMAIL: 'test@example.invalid' },
});
let serverOutput = ''; server.stdout.on('data', data => { serverOutput += data; }); server.stderr.on('data', data => { serverOutput += data; });
const base = `http://127.0.0.1:${port}`;
let browser;
const errors = [], external = [];
try {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server.exitCode !== null) throw new Error(`Server exited: ${serverOutput}`);
    try { const response = await fetch(`${base}/api/health`); if (response.ok) break; } catch {}
    if (attempt === 99) throw new Error(`Server did not start: ${serverOutput}`);
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  console.log('Server ready; launching browser');
  browser = await chromium.launch({ timeout: 15000 });
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  await context.route('**/*', route => {
    if (new URL(route.request().url()).origin !== base) { external.push(route.request().url()); return route.abort(); }
    return route.continue();
  });
  const page = await context.newPage(); page.on('pageerror', error => { errors.push(error.message); console.error('Browser error:', error.message); });
  page.on('console', message => { if (message.type() === 'error') console.error('Browser console:',message.text()); });
  await page.addInitScript(() => localStorage.setItem('gitleaf.panes', '{broken-json'));
  await page.goto(base);
  await page.waitForFunction(() => window.gitleaf?.openNew);
  await page.locator('#ndlg').waitFor({ state: 'visible' });
  await page.locator('#n-name').fill('paper'); await page.locator('#n-create').click();
  await page.waitForFunction(() => gitleaf.state.project === 'paper' && gitleaf.state.file === 'main.tex' && !gitleaf.state.switching);
  assert.equal(await page.locator('#updatebtn').count(), 1);
  console.log('PASS: offline startup, corrupt preferences, template creation and file opening');

  await page.evaluate(() => gitleaf.cm.setValue('\\documentclass{article}\n\\begin{document}\nHello GitLeaf.\n\\end{document}\n'));
  await page.evaluate(() => gitleaf.save());
  assert.match(await readFile(path.join(root, 'paper/main.tex'), 'utf8'), /Hello GitLeaf/);
  assert.equal(await page.locator('#savestate').innerText().then(text => text.startsWith('Saved')), true);
  console.log('PASS: edit and save to disk');

  let release; const gate = new Promise(resolve => { release = resolve; }); let captured;
  const seen = new Promise(resolve => { captured = resolve; });
  const endpoint = '**/api/projects/paper/file?path=main.tex';
  const delaySave = async route => { if (route.request().method() === 'PUT') { captured(); await gate; } await route.continue(); };
  await page.route(endpoint, delaySave);
  await page.evaluate(() => { gitleaf.cm.setValue('first revision'); window.pendingSave = gitleaf.save(); });
  await seen;
  await page.evaluate(() => { gitleaf.cm.setValue('second revision'); });
  release(); assert.equal(await page.evaluate(() => window.pendingSave), true);
  await page.unroute(endpoint, delaySave);
  assert.equal(await readFile(path.join(root, 'paper/main.tex'), 'utf8'), 'second revision');
  console.log('PASS: typing during an in-flight save preserves the newest revision');

  await page.evaluate(() => gitleaf.cm.setValue('\\documentclass{article}\n\\begin{document}\nStable document.\n\\end{document}'));
  await page.locator('#compile').click();
  await page.waitForFunction(() => gitleaf.state.compile?.ok && !gitleaf.state.compileBusy);
  await page.locator('.page canvas').waitFor();
  const oldPdf = Buffer.from(await (await fetch(`${base}/api/projects/paper/pdf`)).arrayBuffer());
  await page.evaluate(() => gitleaf.cm.setValue('\\undefinedcmd'));
  await page.locator('#compile').click();
  await page.waitForFunction(() => gitleaf.state.compile?.ok === false && !gitleaf.state.compileBusy);
  assert.equal(await page.locator('#problems').isVisible(), true);
  assert.deepEqual(Buffer.from(await (await fetch(`${base}/api/projects/paper/pdf`)).arrayBuffer()), oldPdf);
  assert.equal(await page.evaluate(() => gitleaf.state.needsCompile), true);
  await page.locator('#plist tr.p').first().click();
  console.log('PASS: compilation failure keeps the successful PDF and exposes clickable errors');

  await page.evaluate(() => gitleaf.cm.setValue('\\documentclass{article}\n\\begin{document}\nMath $x^2$ stays in LaTeX.\n\\end{document}\n'));
  await page.evaluate(() => gitleaf.enterVisual());
  assert.equal(await page.locator('#visual .katex').count() > 0, true);
  const beforeVisual = await page.evaluate(() => gitleaf.cm.getValue());
  await page.evaluate(() => { gitleaf.syncFromVisual(); gitleaf.exitVisual(); });
  assert.equal(await page.evaluate(() => gitleaf.cm.getValue()), beforeVisual);
  console.log('PASS: offline visual math rendering and untouched LaTeX round-trip');

  await page.evaluate(() => gitleaf.save());
  await page.evaluate(() => gitleaf.setFocus(true));
  assert.equal(await page.locator('#side').isVisible(), false);
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('#side').isVisible(), true);
  await page.locator('#sidebar-toggle').click(); assert.equal(await page.locator('#side').isVisible(), false);
  await page.locator('#sidebar-toggle').click(); assert.equal(await page.locator('#side').isVisible(), true);
  console.log('PASS: Focus Mode and one-click file explorer');

  for (const [width, height] of [[1366,768], [1440,900], [1920,1080], [900,700]]) {
    await page.setViewportSize({ width, height });
    const dimensions = await page.evaluate(() => ({ width: innerWidth, page: document.documentElement.scrollWidth, editor: document.querySelector('#editor').getBoundingClientRect().width, pdf: document.querySelector('#pdfpane').getBoundingClientRect().width }));
    assert.equal(dimensions.page <= dimensions.width, true, JSON.stringify(dimensions));
    assert.equal(dimensions.editor >= 260 && dimensions.pdf >= 260, true, JSON.stringify(dimensions));
    assert.equal(await page.locator('#compile').isVisible(), true);
  }
  await page.setViewportSize({ width:1440, height:900 });
  await page.locator('#compile').click();
  await page.waitForFunction(() => gitleaf.state.compile?.ok && !gitleaf.state.compileBusy);
  await page.locator('.page canvas').waitFor();
  await mkdir(process.env.GITLEAF_SCREENSHOT_DIR || 'test-artifacts', { recursive:true });
  await page.screenshot({ path:path.join(process.env.GITLEAF_SCREENSHOT_DIR || 'test-artifacts', 'gitleaf-desktop.png') });
  console.log('PASS: desktop and laptop layouts (1366, 1440, 1920, 900 px)');

  const response = await fetch(`${base}/api/projects/new`, { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({name:'second',template:'article'}) });
  assert.equal(response.ok,true, await response.text());
  await rename(path.join(root, 'second/main.tex'), path.join(root, 'second/paper.tex'));
  await writeFile(path.join(root, 'second/a.tex'), '% \\documentclass{ignored}\nA chapter, not the main document.');
  await mkdir(path.join(root, 'second/build'));
  await writeFile(path.join(root, 'second/build/main.tex'), '\\documentclass{generated}');
  const fileReads = [];
  await page.route('**/api/projects/second/file?*', async route => {
    if (route.request().method() === 'GET') fileReads.push(new URL(route.request().url()).searchParams.get('path'));
    await route.continue();
  });
  await page.evaluate(() => gitleaf.openProject('second'));
  assert.equal(await page.evaluate(() => gitleaf.state.main), 'paper.tex');
  assert.deepEqual(fileReads, ['paper.tex']);
  console.log('PASS: Rust main-file detection opens a nonstandard main with one file request');

  assert.equal(await page.evaluate(() => gitleaf.pdfv.doc === null), true);
  assert.equal(await page.locator('#pdf-download').getAttribute('href'), null);
  console.log('PASS: changing projects clears the previous PDF and download link');

  assert.deepEqual(errors, []); assert.deepEqual(external, []);
  console.log('PASS: no uncaught browser errors or external browser requests');
} finally {
  if (browser) await browser.close();
  server.kill('SIGTERM');
  if (server.exitCode === null) await once(server, 'exit');
  await rm(directory, {recursive:true,force:true});
}
