// Optional real-engine smoke test. Requires tectonic or latexmk on PATH.
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import path from 'node:path';

const temporary = await mkdtemp(path.join(tmpdir(), 'gitleaf-latex-'));
const socket = createServer(); socket.listen(0, '127.0.0.1'); await once(socket, 'listening');
const port = socket.address().port; await new Promise(resolve => socket.close(resolve));
const binary = process.env.GITLEAF_BIN || path.resolve('target/release/gitleaf');
const server = spawn(binary, ['--no-open', '--port', String(port), '--root', path.join(temporary, 'projects'), '--cache', path.join(temporary, 'builds')], {
  env: { ...process.env, GITLEAF_NO_UPDATE_CHECK: '1', GITLEAF_ENGINE: process.env.GITLEAF_REAL_ENGINE || 'tectonic', TECTONIC_CACHE_DIR: path.join(temporary, 'tex-cache'), GIT_AUTHOR_NAME:'GitLeaf Test', GIT_AUTHOR_EMAIL:'test@example.invalid', GIT_COMMITTER_NAME:'GitLeaf Test', GIT_COMMITTER_EMAIL:'test@example.invalid' },
});
let output = ''; server.stdout.on('data', data => { output += data; }); server.stderr.on('data', data => { output += data; });
const base = `http://127.0.0.1:${port}`;
async function api(endpoint, body) {
  const response = await fetch(base + endpoint, body ? {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)} : {});
  const result = await response.json();
  assert.equal(response.ok, true, JSON.stringify(result));
  return result;
}
async function source(content) {
  const response = await fetch(base + '/api/projects/smoke/file?path=main.tex', {method:'PUT',headers:{'Content-Type':'application/json'},body:JSON.stringify({content})});
  assert.equal(response.ok, true, await response.text());
}
try {
  for (let attempt=0; attempt<100; attempt++) {
    if (server.exitCode !== null) throw new Error(output);
    try { if ((await fetch(base + '/api/health')).ok) break; } catch {}
    await new Promise(resolve => setTimeout(resolve,50));
  }
  await api('/api/projects/new', {name:'smoke',template:'article'});
  await source('\\documentclass{article}\n\\begin{document}\nGitLeaf real engine smoke test. $x^2 + y^2 = z^2$\n\\end{document}\n');
  console.log('Compiling a real document with ' + (process.env.GITLEAF_REAL_ENGINE || 'tectonic'));
  const compiled = await api('/api/projects/smoke/compile',{main:'main.tex'});
  assert.equal(compiled.ok,true,compiled.log);
  const original = Buffer.from(await (await fetch(base + '/api/projects/smoke/pdf')).arrayBuffer());
  assert.equal(original.subarray(0,5).toString(),'%PDF-');
  const forward = await api('/api/projects/smoke/synctex/forward?main=main.tex&file=main.tex&line=3');
  assert.equal(forward.page>0,true);
  const inverse = await api(`/api/projects/smoke/synctex/inverse?main=main.tex&page=${forward.page}&x=${forward.x}&y=${forward.y}`);
  assert.equal(inverse.file,'main.tex');
  await source('\\documentclass{article}\n\\begin{document}\n\\undefinedcmd\n\\end{document}\n');
  const failed = await api('/api/projects/smoke/compile',{main:'main.tex'});
  assert.equal(failed.ok,false); assert.equal(failed.pdf,true);
  assert.equal(failed.problems.some(problem => problem.kind==='error'),true,failed.log);
  const preserved = Buffer.from(await (await fetch(base + '/api/projects/smoke/pdf')).arrayBuffer());
  assert.deepEqual(preserved,original);
  console.log(`PASS: real ${compiled.engine} compilation (${compiled.duration_ms} ms), both SyncTeX directions, actionable errors and previous PDF preservation`);
} finally {
  server.kill('SIGTERM');
  if (server.exitCode === null && server.signalCode === null) await once(server,'exit');
  await rm(temporary,{recursive:true,force:true});
}
