import { SaveQueue } from './save-queue.mjs';
import './styles.css';
import 'katex/dist/katex.min.css';
import { basicSetup } from 'codemirror';
import { EditorState, Compartment } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import { StreamLanguage } from '@codemirror/language';
import { stex } from '@codemirror/legacy-modes/mode/stex';
import { oneDark } from '@codemirror/theme-one-dark';
import * as pdfjsLib from 'pdfjs-dist/build/pdf.mjs';
pdfjsLib.GlobalWorkerOptions.workerSrc = '/assets/pdf.worker.min.mjs';

const $ = s => document.querySelector(s);
const $$ = s => [...document.querySelectorAll(s)];
// KaTeX is only needed by the visual editor, so it loads on first use instead of on every page view.
let katexReady = null;
function ensureKatex() {
  if (!katexReady) katexReady = import('katex').then(module => { window.katex = module.default; }).catch(error => { katexReady = null; throw error; });
  return katexReady;
}
// Preferences must never prevent the editor from opening (private mode, corrupt storage).
const preferences = {
  getItem(key) { try { return localStorage.getItem(key); } catch { return null; } },
  setItem(key, value) { try { localStorage.setItem(key, value); } catch {} },
  removeItem(key) { try { localStorage.removeItem(key); } catch {} },
};

const enc = encodeURIComponent;
function esc(t) { return String(t).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]); }
const state = { project: null, file: null, dirty: false, files: [], loading: false, git: null, visual: false, focus: false, templates: [], tabs: new Map(), lastSave: null, problems: [], compile: null, openDirs: new Set(), closedDirs: new Set(), main: 'main.tex', needsCompile: true, compileBusy: false, switching: false, pulling: false, editVersion: 0 };
// The Compile button is green only while saved files differ from what the PDF was built from.
function setNeedsCompile(v) { state.needsCompile = v; const b = $('#compile'); b.classList.toggle('idle', !v); b.title = v ? 'Save and compile (⌘/Ctrl+Enter)' : 'PDF is up to date with the saved files (⌘/Ctrl+Enter compiles anyway)'; }
window.gitleaf = { state };

async function api(path, opts = {}) {
  const r = await fetch(path, { headers: { 'Content-Type': 'application/json' }, ...opts });
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.error || r.statusText);
  return j;
}
const P = () => `/api/projects/${enc(state.project)}`;
let toastTimer = null;
function toast(msg, kind = '', ms = 2600) { const t = $('#toast'); t.textContent = msg; t.className = 'show ' + kind; clearTimeout(toastTimer); toastTimer = setTimeout(() => t.className = '', ms); }
function setStatus(msg, kind = '') { if (msg) toast(msg, kind); }
function setLog(text, open) { $('#log').textContent = text || ''; if (open) showProblems('logs'); }

// ------------------------------------------------------------------ editor (CodeMirror 6)
function langFor(path) { const e = ((path || '').split('.').pop() || '').toLowerCase(); return ['tex', 'cls', 'sty', 'bib', 'bst', 'dtx', 'ltx'].includes(e) ? StreamLanguage.define(stex) : []; }
const langConf = new Compartment();
const writableConf = new Compartment();
const ourKeys = keymap.of([
  { key: 'Mod-s', run: () => { save(); return true; } },
  { key: 'Mod-Enter', run: () => { compile(); return true; } },
  { key: 'Mod-j', run: () => { forwardSync(); return true; } },
]);
function editorExtensions(path) {
  return [
    ourKeys, basicSetup, EditorView.lineWrapping, oneDark,
    EditorView.contentAttributes.of({ spellcheck: 'true', autocorrect: 'off', autocapitalize: 'off' }),
    langConf.of(langFor(path)), writableConf.of(EditorState.readOnly.of(state.pulling || state.updating || !path)),
    EditorView.updateListener.of(u => {
      if (u.docChanged && !state.loading && state.file) { state.dirty = true; markDirty(); scheduleSave(); }
      if (u.selectionSet || u.docChanged) updatePos();
    }),
  ];
}
const view = new EditorView({ parent: $('#code-host'), state: EditorState.create({ doc: '', extensions: editorExtensions('') }) });
const cm = {
  getValue: () => view.state.doc.toString(),
  setValue: text => view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } }),
  reset: (text, path) => view.setState(EditorState.create({ doc: text, extensions: editorExtensions(path) })),
  focus: () => view.focus(),
  refresh: () => {},
};
function updatePos() { const head = view.state.selection.main.head; const line = view.state.doc.lineAt(head); $('#pos').textContent = `Ln ${line.number}, Col ${head - line.from + 1}`; }
function gotoLine(n) { const doc = view.state.doc; const line = doc.line(Math.max(1, Math.min(doc.lines, n))); view.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: 'center' }) }); view.focus(); }

// ------------------------------------------------------------------ save state
let saveTimer = null, saveTick = null;
function scheduleSave() { clearTimeout(saveTimer); saveTimer = setTimeout(() => save(), 1500); }
function activeTab() { return state.file ? state.tabs.get(state.file) : null; }
function markDirty() { const t = activeTab(); if (t) { t.dirty = state.dirty; t.revision = (t.revision || 0) + 1; t.state = view.state; } state.editVersion++; setNeedsCompile(true); renderTabs(); updateSave(); }
function anyDirty() { return [...state.tabs.values()].some(t => t.dirty); }
function ago(ts) { const s = Math.round((Date.now() - ts) / 1000); if (s < 8) return 'just now'; if (s < 60) return `${s} s ago`; const m = Math.round(s / 60); if (m < 60) return `${m} min ago`; return `${Math.round(m / 60)} h ago`; }
function updateSave(mode) {
  const el = $('#savestate'); if (!state.project) { el.textContent = ''; el.className = ''; return; }
  if (mode === 'saving') { el.textContent = 'Saving…'; el.className = 'saving'; return; }
  if (mode === 'error') { el.textContent = 'Save failed'; el.className = 'err'; el.title = 'Edits are still in the editor; check the log'; return; }
  if (anyDirty()) { el.textContent = '● Unsaved'; el.className = 'unsaved'; el.title = 'Autosaves 1.5 s after you stop typing (⌘S to save now)'; }
  else { el.textContent = state.lastSave ? `Saved ${ago(state.lastSave)}` : 'Saved'; el.className = ''; el.title = 'All edits are on disk. Use Git ▾ to commit and push them.'; }
}
clearInterval(saveTick); saveTick = setInterval(() => { if (!anyDirty()) updateSave(); }, 15000);
const saves = new SaveQueue({
  snapshot() {
    if (state.visual) syncFromVisual();
    if (!state.project) return [];
    return [...state.tabs.entries()].filter(([, tab]) => tab.dirty).map(([path, tab]) => ({
      project: state.project, path, tab, revision: tab.revision || 0,
      content: path === state.file ? cm.getValue() : tab.state.doc.toString(),
    }));
  },
  async write(document) {
    updateSave('saving');
    await api(`/api/projects/${enc(document.project)}/file?path=${enc(document.path)}`, { method: 'PUT', body: JSON.stringify({ content: document.content }) });
  },
  commit(document) {
    if ((document.tab.revision || 0) === document.revision) {
      document.tab.dirty = false;
      if (state.project === document.project && state.file === document.path) state.dirty = false;
    }
    state.lastSave = Date.now();
  },
  failed(document, error) { toast(`Save failed for ${document.path}: ${error.message}`, 'err'); },
});
async function save() {
  clearTimeout(saveTimer);
  const ok = await saves.save();
  renderTabs(); updateSave(ok ? undefined : 'error');
  if (ok) refreshGit();
  return ok;
}

// ------------------------------------------------------------------ projects
async function loadProjects(select) {
  const projects = await api('/api/projects'); state.projects = projects;
  const want = select || state.project || preferences.getItem('gitleaf.project');
  if (want && projects.some(p => p.name === want)) { if (want !== state.project) await openProject(want); }
  else if (!state.project) { $('#projname').textContent = projects.length ? 'Choose a project' : 'New project'; if (!projects.length) openNew(); }
}
async function openProject(name) {
  if (state.switching || state.compileBusy || state.pulling || state.updating) { toast('Wait for the current operation to finish'); return; }
  state.switching = true;
  try {
  if (!(await save())) return;
  if (state.visual) exitVisual();
  state.project = name; state.git = null; renderGit(); state.file = null; state.files = []; state.dirty = false; state.tabs = new Map(); state.openDirs = new Set(); state.closedDirs = new Set(); state.problems = []; state.compile = null; state.lastSave = null;
  preferences.setItem('gitleaf.project', name); $('#filter').value = '';
  $('#projname').textContent = name; document.title = `${name} — gitleaf`;
  resetPdf(); hideProblems(); setNeedsCompile(true);
  cm.reset('', ''); renderTabs(); renderProblems(); setCompileStatus(null);
  await loadFiles(); await guessMain();
  const main = state.files.find(f => f.path === state.main);
  if (main) await openFile(main.path);
  refreshGit(); updateSave();
  await showPdf(); setNeedsCompile(true);
  } finally { state.switching = false; }
}
function projectMenu(btn) {
  const items = [{ hd: 'Projects' }];
  for (const p of state.projects || []) items.push({ label: p.name, ico: p.name === state.project ? '●' : '', on: p.name === state.project, hint: p.dirty ? 'changes' : '', action: () => openProject(p.name) });
  items.push({ sep: true }, { label: 'New / Import project…', ico: '＋', action: () => openNew() });
  if (state.project) items.push({ label: 'Download project as zip', ico: '⤓', action: async () => { if (await save()) window.location.href = `${P()}/zip`; } });
  menu(btn, items);
}

// ------------------------------------------------------------------ files
const GEN_EXT = new Set(['aux', 'log', 'out', 'bbl', 'blg', 'fls', 'fdb_latexmk', 'synctex', 'gz', 'toc', 'lof', 'lot', 'nav', 'snm', 'vrb', 'xdv', 'bcf', 'run.xml', 'idx', 'ind', 'ilg']);
const GEN_DIRS = new Set(['output', 'build', 'tmp', '_minted', '.cache', 'out', 'auxil', '_to_delete']);
function isGenerated(path) { const top = path.split('/')[0]; if (path.includes('/') && GEN_DIRS.has(top)) return true; const e = (path.split('.').pop() || '').toLowerCase(); return GEN_EXT.has(e) || path.endsWith('.synctex.gz') || path.endsWith('.run.xml'); }
function icoFor(path) {
  const e = (path.split('.').pop() || '').toLowerCase();
  if (e === 'tex') return ['tex', 'tex']; if (e === 'bib') return ['bib', 'bib'];
  if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'pdf', 'eps'].includes(e)) return ['img', e];
  if (['cls', 'sty', 'bst', 'bbx', 'cbx', 'dbx', 'def', 'clo'].includes(e)) return ['cls', e];
  return ['', e.slice(0, 4) || '·'];
}
async function loadFiles() {
  if (!state.project) return;
  const project = state.project;
  const files = await api(`${P()}/files`);
  if (state.project !== project) return;
  state.files = files; renderFileTree(); setNeedsCompile(true);
}
function fileRow(f, depth, showPath) {
  const i = f.path.lastIndexOf('/'); const base = i >= 0 ? f.path.slice(i + 1) : f.path;
  const [cls, label] = icoFor(f.path);
  const star = /\.tex$/i.test(f.path) ? `<button class="star" title="${f.path === state.main ? 'This is the main file' : 'Set as main file'}">★</button>` : '';
  return `<div class="f ${f.text ? '' : 'bin'} ${f.path === state.file ? 'active' : ''} ${f.path === state.main ? 'main' : ''}" style="--depth:${depth}" data-path="${esc(f.path)}" title="${esc(f.path)} (${f.size} B)" tabindex="0"><span class="ico ${cls}">${label}</span><span class="nm">${esc(base)}${showPath && i >= 0 ? ` <span class="path">${esc(f.path.slice(0, i))}</span>` : ''}</span>${star}</div>`;
}
function renderTree(files, box) {
  const root = { dirs: Object.create(null), files: [] };
  for (const f of files) { const parts = f.path.split('/'); let n = root; for (const d of parts.slice(0, -1)) n = n.dirs[d] ||= { dirs: Object.create(null), files: [] }; n.files.push(f); }
  const mustOpen = p => (state.file && state.file.startsWith(p + '/')) || (state.main || '').startsWith(p + '/');
  const count = n => n.files.length + Object.values(n.dirs).reduce((a, d) => a + count(d), 0);
  const walk = (n, prefix, depth) => {
    let h = '';
    for (const d of Object.keys(n.dirs).sort((a, b) => a.localeCompare(b))) {
      const p = prefix ? prefix + '/' + d : d; const child = n.dirs[d];
      const open = state.openDirs.has(p) ? true : state.closedDirs.has(p) ? false : (mustOpen(p) || !(d.startsWith('.') || d.startsWith('_')));
      h += `<div class="d ${open ? '' : 'collapsed'}" style="--depth:${depth}" data-dir="${esc(p)}" tabindex="0"><span class="tw">${open ? '▾' : '▸'}</span><span class="nm">${esc(d)}</span><span class="cnt">${count(child)}</span></div><div class="kids">${walk(child, p, depth + 1)}</div>`;
    }
    const fs = n.files.slice().sort((a, b) => { const ta = /\.tex$/i.test(a.path), tb = /\.tex$/i.test(b.path); return (tb - ta) || a.path.localeCompare(b.path); });
    for (const f of fs) h += fileRow(f, depth, false);
    return h;
  };
  box.innerHTML = walk(root, '', 0);
}
function renderFileTree() {
  const q = ($('#filter').value || '').trim().toLowerCase();
  if (q) { const hits = state.files.filter(f => f.path.toLowerCase().includes(q)); $('#files').innerHTML = hits.map(f => fileRow(f, 0, true)).join('') || '<div class="f bin" style="--depth:0">no match</div>'; $('#genfiles').innerHTML = ''; $('#gencount').textContent = ''; return; }
  const gen = state.files.filter(f => isGenerated(f.path)), src = state.files.filter(f => !isGenerated(f.path));
  renderTree(src, $('#files')); renderTree(gen, $('#genfiles'));
  $('#gencount').textContent = gen.length ? `(${gen.length})` : '';
}
$('#filter').addEventListener('input', renderFileTree);
$('#gen-hd').onclick = () => { const g = $('#genfiles'); g.hidden = !g.hidden; $('#gen-hd .tw').textContent = g.hidden ? '▸' : '▾'; };
function texFiles() { return state.files.filter(f => /\.tex$/i.test(f.path)).map(f => f.path).sort((a, b) => (a.includes('/') - b.includes('/')) || a.localeCompare(b)); }
async function guessMain() {
  const stored = preferences.getItem('gitleaf.main.' + state.project); const texs = texFiles();
  if (stored && texs.includes(stored)) { state.main = stored; return; }
  if (texs.includes('main.tex')) { state.main = 'main.tex'; return; }
  for (const p of texs.filter(p => !p.includes('/')).slice(0, 20)) {
    try { const j = await api(`${P()}/file?path=${enc(p)}`); if (/^[^%\n]*\\documentclass/m.test(j.content)) { state.main = p; preferences.setItem('gitleaf.main.' + state.project, p); return; } } catch (e) { /* skip */ }
  }
  state.main = texs[0] || 'main.tex';
}
function setMain(path) { if (state.compileBusy || state.switching || state.pulling || state.updating) { toast('Wait for the current operation to finish'); return; } state.main = path; state.compile = null; state.problems = []; setCompileStatus(null); renderProblems(); resetPdf(); setNeedsCompile(true); preferences.setItem('gitleaf.main.' + state.project, path); renderFileTree(); toast(`Main file: ${path}`, 'ok'); showPdf(); }
async function newFile(initial) {
  if (!state.project) return;
  const path = prompt('New file (path inside the project):', initial || 'sections/new.tex'); if (!path) return;
  try { await api(`${P()}/file?create=true&path=${enc(path)}`, { method: 'PUT', body: JSON.stringify({ content: '' }) }); await loadFiles(); await openFile(path); } catch (e) { toast(`Could not create: ${e.message}`, 'err'); }
}
async function newFolder() {
  if (!state.project) return;
  const folder = prompt('New folder name:', 'figures'); if (!folder) return;
  try { await api(`${P()}/file?create=true&path=${enc(folder.replace(/\/+$/, '') + '/.gitkeep')}`, { method: 'PUT', body: JSON.stringify({ content: '' }) }); state.openDirs.add(folder); await loadFiles(); } catch (e) { toast(`Could not create folder: ${e.message}`, 'err'); }
}
async function uploadFiles(fileList, folder) {
  if (!state.project || !fileList.length) return;
  let n = 0;
  for (const f of fileList) {
    const path = (folder ? folder + '/' : '') + f.name;
    try { const r = await fetch(`${P()}/raw?create=true&path=${enc(path)}`, { method: 'PUT', body: f }); if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error || r.statusText); n++; }
    catch (e) { toast(`Upload failed for ${f.name}: ${e.message}`, 'err'); }
  }
  await loadFiles(); refreshGit(); if (n) toast(`Uploaded ${n} file${n > 1 ? 's' : ''}`, 'ok');
}
async function renameFile(path) {
  const to = prompt('Rename / move to:', path); if (!to || to === path) return;
  if (!(await save())) return;
  try { await api(`${P()}/rename`, { method: 'POST', body: JSON.stringify({ from: path, to }) }); const tab = state.tabs.get(path); if (tab) { state.tabs.delete(path); state.tabs.set(to, tab); if (state.file === path) state.file = to; } if (state.main === path) setMain(to); await loadFiles(); renderTabs(); refreshGit(); }
  catch (e) { toast(`Rename failed: ${e.message}`, 'err'); }
}
async function deleteFile(path) {
  if (!path || !confirm(`Delete ${path}?\nOnly previously committed versions can be recovered from Git.`)) return;
  if (!(await save())) return;
  try { await api(`${P()}/file?path=${enc(path)}`, { method: 'DELETE' }); closeTab(path, true); await loadFiles(); refreshGit(); } catch (e) { toast(`Delete failed: ${e.message}`, 'err'); }
}
$('#files').addEventListener('click', onTreeClick); $('#genfiles').addEventListener('click', onTreeClick);
function onTreeClick(e) {
  const d = e.target.closest('.d');
  if (d) { const p = d.dataset.dir; if (d.classList.contains('collapsed')) { state.openDirs.add(p); state.closedDirs.delete(p); } else { state.closedDirs.add(p); state.openDirs.delete(p); } renderFileTree(); return; }
  const el = e.target.closest('.f'); if (!el) return;
  if (e.target.closest('.star')) { setMain(el.dataset.path); return; }
  openFile(el.dataset.path);
}
for (const id of ['#files', '#genfiles']) $(id).addEventListener('keydown', e => { if (e.key === 'Enter') onTreeClick(e); });
document.addEventListener('contextmenu', e => {
  const el = e.target.closest('#files .f, #genfiles .f'); if (!el) return;
  e.preventDefault(); const path = el.dataset.path; const f = state.files.find(x => x.path === path);
  menu(null, [
    { label: 'Open', action: () => openFile(path), disabled: f && !f.text },
    { label: 'Set as main file', action: () => setMain(path), disabled: !/\.tex$/i.test(path), on: path === state.main },
    { sep: true },
    { label: 'Rename / move…', action: () => renameFile(path) },
    { label: 'Download', action: () => { const a = document.createElement('a'); a.href = `${P()}/raw?path=${enc(path)}`; a.download = path.split('/').pop(); a.click(); } },
    { sep: true },
    { label: 'Delete', action: () => deleteFile(path), danger: true },
  ], { x: e.clientX, y: e.clientY });
});
$('#newfile').onclick = () => newFile(); $('#newfolder').onclick = newFolder;
$('#upload').onclick = () => $('#uploadinput').click(); $('#uploadinput').addEventListener('change', () => { uploadFiles([...$('#uploadinput').files]); $('#uploadinput').value = ''; });
['dragover', 'drop'].forEach(ev => $('#side').addEventListener(ev, e => { if (!e.dataTransfer || ![...e.dataTransfer.types].includes('Files')) return; e.preventDefault(); if (ev === 'drop') uploadFiles([...e.dataTransfer.files]); }));

// ------------------------------------------------------------------ tabs
function base(p) { return p.split('/').pop(); }
function renderTabs() {
  const paths = [...state.tabs.keys()];
  $('#tabs').innerHTML = paths.map(p => { const t = state.tabs.get(p); const [cls, label] = icoFor(p); return `<div role="tab" tabindex="0" aria-selected="${p === state.file}" class="tab ${p === state.file ? 'on' : ''}" data-path="${esc(p)}" title="${esc(p)}"><span class="ico ${cls}">${label}</span>${esc(base(p))}${t.dirty ? '<span class="dot">●</span>' : ''}<button class="x" aria-label="Close ${esc(base(p))}" title="Close (⌘W)">×</button></div>`; }).join('');
  const on = $('#tabs .tab.on'); if (on) on.scrollIntoView({ inline: 'nearest' });
  $('#lang').textContent = state.file ? ({ tex: 'LaTeX', bib: 'BibTeX', md: 'Markdown', cls: 'LaTeX class', sty: 'LaTeX package' }[(state.file.split('.').pop() || '').toLowerCase()] || 'Text') : '';
}
$('#tabs').addEventListener('click', e => { const t = e.target.closest('.tab'); if (!t) return; if (e.target.closest('.x')) closeTab(t.dataset.path); else openFile(t.dataset.path); });
$('#tabs').addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.target.click(); } });
$('#tabs').addEventListener('auxclick', e => { const t = e.target.closest('.tab'); if (t && e.button === 1) { e.preventDefault(); closeTab(t.dataset.path); } });
async function closeTab(path, force) {
  const t = state.tabs.get(path); if (!t) return;
  if (t.dirty && !force) { if (state.file === path && state.visual) syncFromVisual(); if (!(await save())) return; }
  const paths = [...state.tabs.keys()]; const idx = paths.indexOf(path);
  state.tabs.delete(path);
  if (state.file === path) { const next = paths[idx + 1] || paths[idx - 1]; state.file = null; state.dirty = false; if (next) await openFile(next); else { if (state.visual) exitVisual(); cm.reset('', ''); renderTabs(); renderFileTree(); updateSave(); } }
  else renderTabs();
}
let fileLoad = 0;
async function openFile(path, opts = {}) {
  const load = ++fileLoad, project = state.project;
  if (!state.project) return;
  const f = state.files.find(x => x.path === path);
  if (f && !f.text) { toast(`${path} is a binary file; use Download from its context menu`); return; }
  if (state.file === path && state.tabs.has(path)) { if (opts.line) gotoLine(opts.line); else if (!state.visual) cm.focus(); return; }
  if (state.visual) syncFromVisual();
  const cur = activeTab(); if (cur) { cur.state = view.state; cur.dirty = state.dirty; }
  let tab = state.tabs.get(path);
  if (!tab) {
    try { const j = await api(`${P()}/file?path=${enc(path)}`); tab = { state: EditorState.create({ doc: j.content, extensions: editorExtensions(path) }), dirty: false, revision: 0 }; if (load !== fileLoad || state.project !== project) return; state.tabs.set(path, tab); }
    catch (e) { toast(`Could not open ${path}: ${e.message}`, 'err'); return; }
  }
  if (load !== fileLoad || state.project !== project) return;
  if (state.visual) syncFromVisual();
  const previous = activeTab(); if (previous) { previous.state = view.state; previous.dirty = state.dirty; }
  state.file = path; state.dirty = tab.dirty;
  state.loading = true; view.setState(tab.state); view.dispatch({ effects: langConf.reconfigure(langFor(path)) }); state.loading = false;
  if (state.visual) { if (/\.tex$/i.test(path)) renderVisual(); else exitVisual(); }
  renderTabs(); renderFileTree(); updateSave(); updatePos();
  const row = $('#files .f.active, #genfiles .f.active'); if (row) row.scrollIntoView({ block: 'nearest' });
  if (opts.line) gotoLine(opts.line); else if (!state.visual) cm.focus();
}
function setEditorValue(text) { state.loading = true; cm.reset(text, state.file || ''); state.loading = false; if (state.visual) renderVisual(); }

// ------------------------------------------------------------------ compile, problems
function setCompileStatus(r) {
  const el = $('#cstatus');
  if (!r) { el.textContent = ''; el.className = ''; return; }
  if (r.busy) { el.innerHTML = '<span class="spin"></span> Compiling…'; el.className = 'busy'; return; }
  const errs = r.problems.filter(p => p.kind === 'error').length, warns = r.problems.length - errs;
  if (r.ok) { el.textContent = `✓ Compiled successfully · ${(r.duration_ms / 1000).toFixed(1)}s${warns ? ` · ${warns} warning${warns > 1 ? 's' : ''}` : ''}`; el.className = 'ok'; }
  else { el.textContent = `✕ Compilation failed · ${errs ? `${errs} error${errs > 1 ? 's' : ''}` : 'see log'}`; el.className = 'err'; }
}
async function compile() {
  if (!state.project) { toast('Choose or create a project first'); return; }
  if (state.compileBusy || state.switching || state.pulling || state.updating) return;
  state.compileBusy = true; $('#compile').disabled = true;
  let retry = false;
  try {
  if (!(await save())) return;
  const editVersion = state.editVersion;
  const main = state.main; preferences.setItem('gitleaf.main.' + state.project, main);
  setCompileStatus({ busy: true }); $('#compile').disabled = true;
    const r = await api(`${P()}/compile`, { method: 'POST', body: JSON.stringify({ main }) });
    state.compile = r; state.problems = r.problems || []; $('#log').textContent = r.log || '';
    setCompileStatus(r); renderProblems(); setNeedsCompile(!r.ok || state.editVersion !== editVersion || anyDirty());
    if (!r.ok) showProblems('problems', false); else if (!state.problemsManual) hideProblems();
    if (r.ok && r.pdf) await showPdf();
    if (!r.ok && !r.pdf) toast('Compilation failed; no PDF yet', 'err');
  } catch (e) {
    setCompileStatus(null);
    if (/no LaTeX engine/.test(e.message)) {
      if (confirm('No LaTeX installation found. Download Tectonic (a self-contained LaTeX engine, about 30 MB, fetches packages on demand) now?')) {
        toast('Downloading Tectonic…'); setLog('Downloading the latest Tectonic release from GitHub…', true);
        try { const r = await api('/api/engine/install', { method: 'POST' }); setLog(`Installed ${r.version} at ${r.engine}`); retry = true; }
        catch (e2) { toast(`Install failed: ${e2.message}`, 'err'); setLog(e2.message, true); }
      }
    } else { toast(`Compile error: ${e.message}`, 'err'); setLog(e.message, true); }
  }
  finally { state.compileBusy = false; $('#compile').disabled = false; }
  if (retry) return compile();
}
function renderProblems() {
  const ps = state.problems || []; const errs = ps.filter(p => p.kind === 'error').length, warns = ps.length - errs;
  $('#pcount').textContent = errs || ''; $('#wcount').textContent = warns || '';
  if (!ps.length) { $('#plist').innerHTML = `<div class="empty">${state.compile ? (state.compile.ok ? 'No problems. ' : 'The engine reported no structured problems; see Logs. ') : ''}Problems appear here after a compile.</div>`; return; }
  $('#plist').innerHTML = `<table><tr><th>Type</th><th>Line</th><th>File</th><th>Problem</th></tr>` + ps.map((p, i) => `<tr class="p" data-i="${i}" tabindex="0"><td><span class="kind ${p.kind}"></span>${p.kind === 'error' ? 'Error' : 'Warning'}</td><td class="ln">${p.line ?? ''}</td><td class="file">${esc(p.file)}</td><td>${esc(p.message)}</td></tr>`).join('') + '</table>';
}
$('#plist').addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.target.click(); } });
$('#plist').addEventListener('click', e => { const tr = e.target.closest('tr.p'); if (!tr) return; const p = state.problems[+tr.dataset.i]; if (p) jumpTo(p.file, p.line); });
async function jumpTo(file, line) {
  const exists = state.files.find(f => f.path === file) || state.files.find(f => f.path.endsWith('/' + file));
  const path = exists ? exists.path : state.main;
  if (state.visual) exitVisual();
  await openFile(path, { line: line || 1 });
}
function showProblems(tab, manual = true) { if (manual) state.problemsManual = true; $('#problems').hidden = false; $('#problems').classList.toggle('logs', tab === 'logs'); $$('#problems .ptab').forEach(b => b.classList.toggle('on', b.dataset.p === tab)); }
function hideProblems() { state.problemsManual = false; $('#problems').hidden = true; }
$$('#problems .ptab').forEach(b => b.onclick = () => showProblems(b.dataset.p));
$('#pclose').onclick = hideProblems;
$('#cstatus').onclick = () => $('#problems').hidden ? showProblems(state.problems.length ? 'problems' : 'logs') : hideProblems();

// ------------------------------------------------------------------ PDF viewer (pdf.js)
const pdfv = { doc: null, mode: 'width', scale: 1, pages: [], current: 1, url: null, searchHits: [], searchIdx: -1, searchQuery: '' };
const pdfScroll = $('#pdfscroll'), pdfPages = $('#pdfpages');
function effScale() {
  if (!pdfv.doc || !pdfv.pages.length) return 1;
  const avail = pdfScroll.clientWidth - 28; const w = pdfv.pages[0].w, h = pdfv.pages[0].h;
  if (pdfv.mode === 'width') return Math.max(0.2, avail / w);
  if (pdfv.mode === 'page') return Math.max(0.2, Math.min(avail / w, (pdfScroll.clientHeight - 28) / h));
  return pdfv.scale;
}
async function showPdf() {
  if (!state.project) return;
  const base = `${P()}/pdf?main=${enc(state.main)}`;
  const dl = $('#pdf-download');
  const project = state.project, main = state.main;
  const ok = await loadPdf(`${base}&t=${Date.now()}`);
  if (project !== state.project || main !== state.main) return;
  if (ok) { dl.href = `${base}&download=true`; dl.classList.remove('off'); }
  else { dl.classList.add('off'); dl.removeAttribute('href'); if (!pdfv.doc) $('#pdf-empty').style.display = ''; }
}
let pdfLoad = 0;
function resetPdf() {
  pdfLoad++;
  if (io) io.disconnect();
  for (const page of pdfv.pages) page.renderTask?.cancel();
  if (pdfv.doc) pdfv.doc.loadingTask.destroy().catch(() => {});
  Object.assign(pdfv, { doc: null, pages: [], current: 1, searchHits: [], searchIdx: -1, searchQuery: '' });
  pdfPages.replaceChildren(); pdfScroll.scrollTop = 0;
  $('#pagecount').textContent = '/ –'; $('#pageno').value = '1'; $('#searchinfo').textContent = ''; $('#pdfsearch').value = '';
  $('#pdf-download').classList.add('off'); $('#pdf-download').removeAttribute('href'); $('#pdf-empty').style.display = '';
}
async function loadPdf(url) {
  const generation = ++pdfLoad;
  const pageNumber = pdfv.current;
  const prior = pdfv.pages[pageNumber - 1];
  const offset = prior?.el ? (pdfScroll.scrollTop - prior.el.offsetTop) / prior.el.offsetHeight : 0;
  let doc;
  try {
    doc = await pdfjsLib.getDocument({ url, isEvalSupported: false, cMapUrl: '/assets/cmaps/', cMapPacked: true, standardFontDataUrl: '/assets/standard_fonts/', wasmUrl: '/assets/wasm/', iccUrl: '/assets/iccs/' }).promise;
    const pages = await Promise.all(Array.from({ length: doc.numPages }, (_, i) => doc.getPage(i + 1).then(page => {
      const viewport = page.getViewport({ scale: 1 }); return { page, w: viewport.width, h: viewport.height, rendered: 0 };
    })));
    if (generation !== pdfLoad) { await doc.loadingTask.destroy(); return false; }
    for (const page of pdfv.pages) page.renderTask?.cancel();
    if (pdfv.doc) await pdfv.doc.loadingTask.destroy();
    if (generation !== pdfLoad) { await doc.loadingTask.destroy(); return false; }
    pdfv.doc = doc; pdfv.pages = pages; pdfv.searchHits = []; pdfv.searchIdx = -1; $('#searchinfo').textContent = '';
    pdfv.current = Math.min(pageNumber, doc.numPages); $('#pageno').value = pdfv.current;
    $('#pagecount').textContent = `/ ${doc.numPages}`; $('#pdf-empty').style.display = 'none';
    layoutPages();
    const current = pdfv.pages[pdfv.current - 1].el;
    pdfScroll.scrollTop = current.offsetTop + offset * current.offsetHeight;
    if (pdfv.searchQuery) pdfSearch(pdfv.searchQuery, true);
    return true;
  } catch (error) {
    if (doc && doc !== pdfv.doc) await doc.loadingTask.destroy();
    if (generation === pdfLoad && !/404|Missing PDF|UnexpectedResponse/i.test(String(error.message || error.name))) toast(`PDF could not be loaded: ${error.message}`, 'err');
    return false;
  }
}

let io = null;
function layoutPages() {
  const s = effScale(); $('#zoomlbl').textContent = Math.round(s * 100) + '%';
  for (const page of pdfv.pages) page.renderTask?.cancel();
  pdfPages.innerHTML = '';
  if (io) io.disconnect();
  io = new IntersectionObserver(entries => { for (const en of entries) { const p = pdfv.pages[+en.target.dataset.page - 1]; if (en.isIntersecting) renderPage(+en.target.dataset.page); else if (p && p.el === en.target) { p.renderTask?.cancel(); p.el.querySelector('canvas')?.remove(); p.rendered = 0; } } }, { root: pdfScroll, rootMargin: '600px 0px' });
  pdfv.pages.forEach((p, i) => {
    const div = document.createElement('div'); div.className = 'page'; div.dataset.page = i + 1; div.style.width = Math.floor(p.w * s) + 'px'; div.style.height = Math.floor(p.h * s) + 'px';
    const num = document.createElement('span'); num.className = 'num'; num.textContent = i + 1; div.appendChild(num);
    p.el = div; p.rendered = 0; p.renderTask = null; pdfPages.appendChild(div); io.observe(div);
  });
}
async function renderPage(n) {
  const p = pdfv.pages[n - 1]; if (!p) return; const s = effScale(); const element = p.el;
  if (p.rendered === s) return; p.rendered = s;
  const vp = p.page.getViewport({ scale: s }); const dpr = Math.min(window.devicePixelRatio || 1, 2, Math.sqrt(16_000_000 / (vp.width * vp.height)));
  const canvas = document.createElement('canvas'); canvas.width = Math.floor(vp.width * dpr); canvas.height = Math.floor(vp.height * dpr); canvas.style.width = Math.floor(vp.width) + 'px'; canvas.style.height = Math.floor(vp.height) + 'px';
  const ctx = canvas.getContext('2d'); ctx.scale(dpr, dpr);
  let task;
  try { task = p.page.render({ canvas, canvasContext: ctx, viewport: vp }); p.renderTask = task; await task.promise; } catch (e) { if (p.renderTask === task) p.rendered = 0; if (e.name !== 'RenderingCancelledException') toast(`PDF render failed: ${e.message}`, 'err'); return; }
  if (p.rendered !== s || p.el !== element) return;
  const old = p.el.querySelector('canvas'); if (old) old.remove(); p.el.insertBefore(canvas, p.el.firstChild);
}
function setZoom(mode, scale) { pdfv.mode = mode; if (scale) pdfv.scale = scale; $('#zoommode').value = mode; const ratio = pdfScroll.scrollHeight > 0 ? pdfScroll.scrollTop / pdfScroll.scrollHeight : 0; layoutPages(); pdfScroll.scrollTop = ratio * pdfScroll.scrollHeight; }
$('#zoomin').onclick = () => setZoom('custom', Math.min(4, effScale() * 1.15)); $('#zoomout').onclick = () => setZoom('custom', Math.max(0.2, effScale() / 1.15));
$('#zoommode').onchange = () => setZoom($('#zoommode').value, pdfv.mode === 'custom' ? pdfv.scale : effScale());
let resizeTimer = null; new ResizeObserver(() => { if (!pdfv.doc || pdfv.mode === 'custom') return; clearTimeout(resizeTimer); resizeTimer = setTimeout(() => setZoom(pdfv.mode), 120); }).observe(pdfScroll);
function pageTop(n) { const el = pdfv.pages[n - 1] && pdfv.pages[n - 1].el; return el ? el.offsetTop : 0; }
function gotoPage(n, yFrac) { if (!pdfv.doc) return; n = Math.max(1, Math.min(pdfv.doc.numPages, n | 0)); const el = pdfv.pages[n - 1].el; pdfScroll.scrollTop = el.offsetTop - 12 + (yFrac ? yFrac * el.offsetHeight : 0); }
pdfScroll.addEventListener('scroll', () => { if (!pdfv.doc) return; const y = pdfScroll.scrollTop + pdfScroll.clientHeight * 0.3; let cur = 1; for (const p of pdfv.pages) { if (p.el && p.el.offsetTop <= y) cur = +p.el.dataset.page; } if (cur !== pdfv.current) { pdfv.current = cur; $('#pageno').value = cur; } });
$('#pgprev').onclick = () => gotoPage(pdfv.current - 1); $('#pgnext').onclick = () => gotoPage(pdfv.current + 1);
$('#pageno').addEventListener('change', () => gotoPage(+$('#pageno').value)); $('#pageno').addEventListener('keydown', e => { if (e.key === 'Enter') { gotoPage(+$('#pageno').value); e.target.blur(); } });
$('#pdffull').onclick = () => { const el = $('#pdfpane'); if (document.fullscreenElement) document.exitFullscreen(); else el.requestFullscreen && el.requestFullscreen(); };
async function pdfSearch(q, keep) {
  pdfv.searchQuery = q; if (!pdfv.doc) return;
  if (!q) { pdfv.searchHits = []; pdfv.searchIdx = -1; $('#searchinfo').textContent = ''; return; }
  if (!keep || !pdfv.searchHits.length) {
    const needle = q.toLowerCase(); const hits = [];
    for (let i = 1; i <= pdfv.doc.numPages; i++) { const tc = await pdfv.pages[i - 1].page.getTextContent(); const text = tc.items.map(it => it.str).join(' ').toLowerCase(); let idx = -1, n = 0; while ((idx = text.indexOf(needle, idx + 1)) >= 0 && n < 50) { hits.push({ page: i, frac: Math.min(0.95, idx / Math.max(1, text.length)) }); n++; } }
    pdfv.searchHits = hits; pdfv.searchIdx = -1;
  }
  if (!pdfv.searchHits.length) { $('#searchinfo').textContent = 'no matches'; return; }
  pdfv.searchIdx = (pdfv.searchIdx + 1) % pdfv.searchHits.length; const h = pdfv.searchHits[pdfv.searchIdx];
  $('#searchinfo').textContent = `${pdfv.searchIdx + 1}/${pdfv.searchHits.length} · p.${h.page}`; gotoPage(h.page, h.frac * 0.9);
}
$('#pdfsearch').addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); pdfSearch($('#pdfsearch').value.trim(), e.target.value.trim() === pdfv.searchQuery); } if (e.key === 'Escape') { e.target.value = ''; pdfSearch(''); } });
$('#pdfsearch').addEventListener('input', () => { if (!$('#pdfsearch').value.trim()) pdfSearch(''); });
// SyncTeX: source -> PDF (⌘J / status-bar button) and PDF -> source (double-click a page)
function flashMark(page, yBp) { const p = pdfv.pages[page - 1]; if (!p || !p.el) return; const s = effScale(); const m = $('#syncmark'); m.style.top = (p.el.offsetTop + yBp * s) + 'px'; m.style.opacity = '1'; setTimeout(() => m.style.opacity = '0', 1800); }
async function forwardSync() {
  if (!state.project || !state.file || !pdfv.doc) { toast('Compile first, then ⌘J jumps from the cursor to the PDF'); return; }
  const line = view.state.doc.lineAt(view.state.selection.main.head).number;
  try { const r = await api(`${P()}/synctex/forward?main=${enc(state.main)}&file=${enc(state.file)}&line=${line}`); const p = pdfv.pages[r.page - 1]; if (!p) return; const s = effScale(); pdfScroll.scrollTop = p.el.offsetTop - 12 + Math.max(0, r.y * s - pdfScroll.clientHeight * 0.35); flashMark(r.page, r.y); }
  catch (e) { toast(e.message, 'err'); }
}
$('#syncbtn').onclick = forwardSync;
pdfPages.addEventListener('dblclick', async e => {
  const pg = e.target.closest('.page'); if (!pg || !state.project) return;
  const rect = pg.getBoundingClientRect(); const s = effScale(); const x = (e.clientX - rect.left) / s, y = (e.clientY - rect.top) / s;
  try { const r = await api(`${P()}/synctex/inverse?main=${enc(state.main)}&page=${pg.dataset.page}&x=${x.toFixed(1)}&y=${y.toFixed(1)}`); await jumpTo(r.file, r.line); toast(`${r.file}:${r.line}`); } catch (err) { toast(err.message, 'err'); }
});

// ------------------------------------------------------------------ git
async function refreshGit() {
  if (!state.project) { state.git = null; renderGit(); return; }
  const project = state.project;
  try { const git = await api(`${P()}/git/status`); if (state.project !== project) return; state.git = git; } catch (e) { if (state.project !== project) return; state.git = null; }
  renderGit();
}
function renderGit() {
  const s = state.git; $('#branch').textContent = s && s.branch ? `⎇ ${s.branch}` : '';
  const n = s ? s.changes.length + s.ahead + s.behind : 0; $('#gitbadge').textContent = n || '';
  $('#gitbtn').title = s ? `${s.changes.length} uncommitted change${s.changes.length === 1 ? '' : 's'} · ${s.ahead} to push · ${s.behind} to pull` : 'Git';
}
setInterval(() => { if (state.project && !document.hidden) refreshGit(); }, 60000);
function gitMenu(btn) {
  const s = state.git; const items = [];
  if (s) {
    const lines = [];
    lines.push(anyDirty() ? '● Unsaved edits in the editor (autosaving)' : 'Editor: all edits saved to disk');
    lines.push(s.changes.length ? `${s.changes.length} file${s.changes.length > 1 ? 's' : ''} changed, not committed` : 'Working tree: nothing to commit');
    lines.push(s.remote ? (s.ahead ? `${s.ahead} commit${s.ahead > 1 ? 's' : ''} not pushed` : 'No unpushed commits (last known remote)' ) + (s.behind ? ` · ${s.behind} to pull` : '') : 'No remote configured');
    items.push({ info: lines.join('\n') }, { sep: true });
  }
  items.push({ label: 'Commit…', ico: '✓', hint: s && s.changes.length ? `${s.changes.length} changed` : '', action: openCommit, disabled: !state.project });
  items.push({ label: 'Push', ico: '↑', hint: s && s.ahead ? `${s.ahead}` : '', action: doPush, disabled: !state.project || !(s && s.remote) });
  items.push({ label: 'Pull', ico: '↓', hint: s && s.behind ? `${s.behind}` : '', action: doPull, disabled: !state.project || !(s && s.remote) });
  items.push({ sep: true }, { label: 'History', ico: '🕘', action: showHistory, disabled: !state.project });
  if (s && s.remote) items.push({ info: `remote: ${s.remote}` });
  menu(btn, items);
}
async function openCommit() {
  if (!state.project) return; if (!(await save())) return; await refreshGit(); const s = state.git; if (!s) return;
  $('#dlg-info').textContent = `${s.branch || '?'} → ${s.remote || 'no remote'} · ${s.ahead} to push, ${s.behind} to pull\nlast commit: ${s.last_commit || '-'}`;
  $('#changes').textContent = s.changes.join('\n') || '(nothing to commit)';
  $('#dlg-commit').disabled = $('#dlg-commitpush').disabled = s.changes.length === 0; $('#dlg-commitpush').disabled = $('#dlg-commitpush').disabled || !s.remote;
  $('#msg').value = ''; $('#dlg').showModal(); $('#msg').focus();
}
async function doCommit(push) {
  const message = $('#msg').value.trim(); if (!message) { $('#msg').focus(); return; } $('#dlg').close();
  try { const r = await api(`${P()}/git/commit`, { method: 'POST', body: JSON.stringify({ message }) }); setLog(r.stdout + r.stderr, !r.ok); if (!r.ok) toast('Commit failed; see Logs', 'err'); else { toast('Committed', 'ok'); if (push) await doPush(); } }
  catch (e) { toast(`Commit error: ${e.message}`, 'err'); }
  await refreshGit(); loadProjects();
}
async function doPush() { if (!state.project) return; toast('Pushing…'); try { const r = await api(`${P()}/git/push`, { method: 'POST' }); setLog(r.stdout + r.stderr, !r.ok); toast(r.ok ? 'Pushed' : 'Push failed; see Logs', r.ok ? 'ok' : 'err'); } catch (e) { toast(`Push error: ${e.message}`, 'err'); } refreshGit(); }
async function doPull() {
  if (!state.project || state.pulling || state.compileBusy || state.switching) return;
  state.pulling = true;
  if (state.visual) exitVisual();
  view.dispatch({ effects: writableConf.reconfigure(EditorState.readOnly.of(true)) });
  try {
    if (!(await save())) return;
    toast('Pulling…');
    const result = await api(`${P()}/git/pull`, { method: 'POST' });
    setLog(result.stdout + result.stderr, !result.ok);
    toast(result.ok ? 'Pulled' : 'Pull failed; see Logs', result.ok ? 'ok' : 'err');
    if (!result.ok) return;
    setNeedsCompile(true); await loadFiles();
    for (const [path, tab] of state.tabs) {
      if (tab.dirty) continue;
      if (!state.files.some(file => file.path === path)) { await closeTab(path, true); continue; }
      const document = await api(`${P()}/file?path=${enc(path)}`);
      tab.state = EditorState.create({ doc: document.content, extensions: editorExtensions(path) });
      if (path === state.file) setEditorValue(document.content);
    }
  } catch (error) { toast(`Pull error: ${error.message}`, 'err'); }
  finally {
    state.pulling = false;
    view.dispatch({ effects: writableConf.reconfigure(EditorState.readOnly.of(false)) });
    refreshGit();
  }
}

async function showHistory() { if (!state.project) return; try { const l = await api(`${P()}/git/log`); $('#hist').textContent = l.map(e => `${e.hash}  ${e.date}  ${e.author.padEnd(16).slice(0, 16)}  ${e.subject}`).join('\n') || '(no commits)'; $('#hdlg').showModal(); } catch (e) { toast(`History: ${e.message}`, 'err'); } }

// ------------------------------------------------------------------ menus, view, focus
let openMenu = null;
function closeMenu() { if (openMenu) { openMenu.remove(); openMenu = null; } }
function menu(btn, items, at) {
  closeMenu();
  const m = document.createElement('div'); m.className = 'menu'; m.setAttribute('role', 'menu');
  for (const it of items) {
    if (it.sep) { const s = document.createElement('div'); s.className = 'sep'; m.appendChild(s); continue; }
    if (it.hd) { const h = document.createElement('div'); h.className = 'hd'; h.textContent = it.hd; m.appendChild(h); continue; }
    if (it.info) { const h = document.createElement('div'); h.className = 'info'; h.textContent = it.info; m.appendChild(h); continue; }
    const b = document.createElement('button'); b.className = 'mi' + (it.on ? ' on' : ''); b.setAttribute('role', 'menuitem'); if (it.disabled) b.disabled = true; if (it.danger) b.style.color = 'var(--err)';
    b.innerHTML = `<span class="ico">${it.ico || ''}</span><span>${esc(it.label)}</span>${it.hint ? `<span class="k">${esc(it.hint)}</span>` : ''}`;
    b.onclick = async () => { closeMenu(); try { await it.action?.(); } catch (error) { toast(error.message, 'err'); } }; m.appendChild(b);
  }
  document.body.appendChild(m); openMenu = m;
  const r = btn ? btn.getBoundingClientRect() : { left: at.x, bottom: at.y, right: at.x };
  let x = r.left, y = r.bottom + 4; const mw = m.offsetWidth, mh = m.offsetHeight;
  if (x + mw > window.innerWidth - 8) x = Math.max(8, (btn ? r.right : at.x) - mw); if (y + mh > window.innerHeight - 8) y = Math.max(8, (btn ? r.top : at.y) - mh - 4);
  m.style.left = x + 'px'; m.style.top = y + 'px';
  const first = m.querySelector('.mi:not([disabled])'); if (first) first.focus();
}
document.addEventListener('mousedown', e => { if (openMenu && !openMenu.contains(e.target)) closeMenu(); });
document.addEventListener('keydown', e => { if (e.key === 'Escape') { if (openMenu) { closeMenu(); return; } if (state.focus) { setFocus(false); return; } } });
// ---- updates: check GitHub releases on start and every six hours; one click downloads and restarts.
async function checkUpdate(force) {
  try {
    const u = await api(`/api/update/check${force ? '?force=1' : ''}`); state.update = u;
    const b = $('#updatebtn');
    if (u.update_available) { b.hidden = false; b.textContent = `⤓ Update to ${u.latest}`; }
    else b.hidden = true;
    return u;
  } catch (e) { return null; }
}
async function applyUpdate() {
  const u = state.update || await checkUpdate(true); if (!u) return;
  if (!u.update_available) { toast(u.error ? `Update check: ${u.error}` : `You are on the latest version (${u.current})`, u.error ? 'err' : 'ok', 4000); return; }
  if (!u.can_self_update) { if (confirm(`gitleaf ${u.latest} is available. ${u.manual_update_instructions || 'Download the new version from the release page.'} Open the release page?`)) window.open(u.url, '_blank', 'noopener'); return; }
  const notes = (u.notes || '').split('\n').slice(0, 12).join('\n');
  if (!confirm(`Update gitleaf ${u.current} → ${u.latest}?\n\nThe new version is downloaded, its checksum verified, and gitleaf restarts (about 10 seconds). Unsaved edits are saved first.\n\n${notes}`)) return;
  if (state.compileBusy || state.switching || state.pulling || state.updating) { toast('Wait for the current operation to finish'); return; }
  if (state.visual) exitVisual();
  state.updating = true;
  view.dispatch({ effects: writableConf.reconfigure(EditorState.readOnly.of(true)) });
  if (!(await save())) { state.updating = false; view.dispatch({ effects: writableConf.reconfigure(EditorState.readOnly.of(false)) }); return; }
  toast('Downloading the update…', '', 60000);
  try {
    const r = await api('/api/update/apply', { method: 'POST' });
    toast(`Installed ${r.to}; restarting…`, 'ok', 60000);
    const started = Date.now();
    const poll = async () => {
      try { const h = await (await fetch('/api/health', { cache: 'no-store' })).json(); if (h.version === r.to) { location.reload(); return; } } catch (e) { /* still restarting */ }
      if (Date.now() - started < 90000) setTimeout(poll, 1500); else toast('The new version did not come back up; start gitleaf again by hand.', 'err', 20000);
    };
    setTimeout(poll, 2500);
  } catch (e) { state.updating = false; view.dispatch({ effects: writableConf.reconfigure(EditorState.readOnly.of(false)) }); toast(`Update failed: ${e.message}`, 'err', 10000); }
}
$('#updatebtn').onclick = applyUpdate;
setTimeout(() => checkUpdate(false), 4000); setInterval(() => checkUpdate(false), 6 * 3600 * 1000);
function viewMenu(btn) {
  const h = state.health || {};
  menu(btn, [
    { label: state.visual ? 'LaTeX source' : 'Visual editor', ico: state.visual ? '</>' : '¶', hint: '', action: () => state.visual ? exitVisual() : enterVisual() },
    { label: 'Focus mode', ico: '◎', hint: '⌘⇧F', on: state.focus, action: () => setFocus(!state.focus) },
    { label: document.body.classList.contains('nosidebar') ? 'Show file explorer' : 'Hide file explorer', ico: '▤', hint: '⌘B', action: () => toggleSidebar() },
    { sep: true },
    { label: 'Problems', ico: '!', action: () => showProblems('problems') },
    { label: 'Compile log', ico: '≡', action: () => showProblems('logs') },
    { sep: true },
    { label: 'Reset panel layout', ico: '⟲', action: () => { $('main').style.removeProperty('--sidew'); $('main').style.removeProperty('--pdfw'); preferences.removeItem('gitleaf.panes'); } },
    { sep: true },
    { label: state.update?.update_available ? `Update to gitleaf ${state.update.latest}` : 'Check for updates', ico: '⤓', action: async () => { await checkUpdate(true); await applyUpdate(); } },
    { label: 'Download gitleaf for Mac, Windows, Linux', ico: '⤓', action: () => window.open('https://github.com/kmarkiv/gitleaf/releases/latest', '_blank', 'noopener') },
    { label: 'Report a problem on GitHub', ico: '↗', action: () => window.open('https://github.com/kmarkiv/gitleaf/issues', '_blank', 'noopener') },
    { info: `Engine: ${h.engine ? h.engine.kind : 'none found (Compile offers Tectonic)'}\ngitleaf ${h.version || ''} · ⌘S save · ⌘Enter compile · ⌘J go to PDF` },
  ]);
}
function toggleSidebar(force) { const on = force !== undefined ? force : !document.body.classList.contains('nosidebar'); document.body.classList.toggle('nosidebar', on); preferences.setItem('gitleaf.nosidebar', on ? '1' : '0'); }
function setFocus(on) { state.focus = on; document.body.classList.toggle('focus', on); if (!on && preferences.getItem('gitleaf.nosidebar') !== '1') document.body.classList.remove('nosidebar'); setTimeout(() => { if (pdfv.doc) setZoom(pdfv.mode); }, 50); }
$('#projbtn').onclick = e => projectMenu(e.currentTarget);
$('#newbtn').onclick = () => openNew();
$('#n-cancel').onclick = () => { dmsg(''); $('#ndlg').close(); };
$('#n-create').onclick = createProject;
$('#ndlg').addEventListener('cancel', () => dmsg('')); $('#gitbtn').onclick = e => gitMenu(e.currentTarget); $('#viewbtn').onclick = e => viewMenu(e.currentTarget); $('#histbtn').onclick = showHistory;
$('#sidebar-toggle').onclick = () => toggleSidebar();
$('#focusexit').onclick = () => setFocus(false);
$('#compile').onclick = compile;
$('#modebtn').onclick = () => state.visual ? exitVisual() : enterVisual();
$('#dlg-cancel').onclick = () => $('#dlg').close(); $('#dlg-commit').onclick = () => doCommit(false); $('#dlg-commitpush').onclick = () => doCommit(true);
$('#hdlg-close').onclick = () => $('#hdlg').close();
$('#msg').addEventListener('keydown', e => { if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') doCommit(false); });
document.addEventListener('keydown', e => {
  const mod = e.metaKey || e.ctrlKey; if (!mod || e.defaultPrevented || (e.target.closest('dialog') && e.key.toLowerCase() !== 's')) return;
  const k = e.key.toLowerCase();
  if (k === 's') { e.preventDefault(); save(); }
  else if (k === 'enter' && !$('#dlg').open && !$('#ndlg').open && !state.visual) { e.preventDefault(); compile(); }
  else if (k === 'b' && !state.visual) { e.preventDefault(); toggleSidebar(); }
  else if (k === 'f' && e.shiftKey) { e.preventDefault(); setFocus(!state.focus); }
  else if (k === 'p' && e.shiftKey) { e.preventDefault(); toggleSidebar(false); $('#filter').focus(); $('#filter').select(); }
  else if (k === 'w' && state.file) { e.preventDefault(); closeTab(state.file); }
  else if (k === 'j') { e.preventDefault(); forwardSync(); }
});
window.addEventListener('beforeunload', e => { if (anyDirty()) { e.preventDefault(); e.returnValue = ''; } });
setInterval(() => { if (anyDirty()) save(); }, 20000);
if (preferences.getItem('gitleaf.nosidebar') === '1') document.body.classList.add('nosidebar');

// ---- new project / import
const N = { tab: 'templates', template: 'article', zipFile: null, olZip: null, loadedTemplates: false };
const OL_TAGS = ['acm', 'ieee', 'springer', 'elsevier', 'thesis', 'cv', 'poster', 'presentation', 'journal', 'conference', 'article', 'book', 'report', 'homework', 'letter', 'newsletter', 'project-proposal', 'academic-journal', 'formal-letter', 'lab-report'];
function showTab(tab) {
  N.tab = tab;
  document.querySelectorAll('#ndlg .tabs button').forEach(b => b.classList.toggle('on', b.dataset.tab === tab));
  document.querySelectorAll('#ndlg .pane').forEach(p => p.hidden = p.dataset.pane !== tab);
  $('#n-create').textContent = tab === 'templates' ? 'Create' : tab === 'git' ? 'Clone' : 'Import';
  if (tab === 'overleaf') olSteps();
}
async function openNew(tab) {
  if (!N.loadedTemplates) {
    try { state.templates = await api('/api/templates'); } catch (e) { setStatus(e.message, 'err'); return; }
    N.loadedTemplates = true;
    $('#n-cards').innerHTML = state.templates.map(t => `<div class="card ${t.id === N.template ? 'on' : ''}" data-id="${t.id}"><div class="thumb"><span class="big">${esc(t.name.split(' ')[0])}</span></div><div class="body"><div class="t">${esc(t.name)}</div><div class="d">${esc(t.description)}</div></div></div>`).join('');
    $('#ol-tags').innerHTML = OL_TAGS.map(t => `<button data-tag="${t}">${t}</button>`).join('');
  }
  N.zipFile = null; N.olZip = null; N.olSel = null; N.olSkipGithub = false; $('#ol-cands').hidden = true; $$('#ol-cards .card').forEach(x => x.classList.remove('on'));
  ['#n-name', '#n-git', '#n-url', '#n-remote', '#ol-link'].forEach(x => $(x).value = ''); olSteps();
  $('#n-drop').classList.remove('has'); $('#n-drop').firstChild.textContent = 'Drop a zip here, or click to choose';
  $('#ol-drop').classList.remove('has'); $('#ol-drop').firstChild.textContent = 'Drop the Overleaf source zip here, or click to choose';
  dmsg(''); showTab(tab || 'templates');
  $('#ndlg').showModal(); $('#n-name').focus();
}
async function browseGallery(tag) {
  tag = (tag || '').trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  if (!tag) return;
  $('#ol-q').value = tag;
  document.querySelectorAll('#ol-tags button').forEach(b => b.classList.toggle('on', b.dataset.tag === tag));
  $('#ol-cards').innerHTML = '<div class="muted">Loading…</div>';
  try {
    const items = await api(`/api/overleaf/gallery?tag=${enc(tag)}`);
    if (!items.length) { $('#ol-cards').innerHTML = `<div class="muted">No Overleaf tag "${esc(tag)}". Try one of the tags above, or search the gallery at overleaf.com/latex/templates and paste the project link below.</div>`; return; }
    N.olItems = items; N.olSel = null; olSteps();
    $('#ol-cards').innerHTML = items.map((it, i) => `<div class="card ol" data-i="${i}" tabindex="0"><div class="thumb">${it.thumb ? `<img src="${esc(it.thumb)}" alt="" loading="lazy">` : ''}</div><div class="body"><div class="t">${esc(it.title)}</div><div class="d">${esc(it.description)}</div><div class="act"><a href="${esc(it.url)}" target="_blank" rel="noopener" title="Opens the template page; press Open as Template there">Open in Overleaf ↗</a><a href="${esc(it.url)}.pdf" target="_blank" rel="noopener">Preview PDF</a></div></div></div>`).join('');
  } catch (e) { $('#ol-cards').innerHTML = `<div class="muted">${esc(e.message)}</div>`; }
}
function guessName(s) { return (s.split('?')[0].replace(/\/+$/, '').split(/[\/:]/).pop() || '').replace(/\.(zip|git)$/i, '').replace(/[^A-Za-z0-9._-]/g, '-'); }
const OL_PROJECT = /https?:\/\/(?:www\.)?overleaf\.com\/project\/[0-9a-f]{24}/i;
function olSteps() {
  const link = OL_PROJECT.test($('#ol-link').value); const picked = !!N.olSel;
  $('#ol-s1').className = 'step ' + (picked || link ? 'done' : 'on'); $('#ol-s2').className = 'step ' + (link ? 'done' : picked ? 'on' : ''); $('#ol-s3').className = 'step ' + (link ? 'on' : '');
  if (N.tab === 'overleaf') $('#n-create').textContent = 'Import';
}
$('#ol-cards').addEventListener('click', e => {
  if (e.target.closest('a')) return;
  const c = e.target.closest('.card.ol'); if (!c) return;
  N.olSel = N.olItems[+c.dataset.i]; $$('#ol-cards .card').forEach(x => x.classList.toggle('on', x === c)); $('#ol-cands').hidden = true;
  if (!$('#n-name').value) { const slug = guessName(N.olSel.url.split('/').slice(-2, -1)[0] || '').replace(/-template$/, ''); $('#n-name').value = slug.length > 40 ? slug.slice(0, 40).replace(/-[^-]*$/, '') : slug; }
  olSteps(); dmsg('');
});
$('#ol-link').addEventListener('input', olSteps);
// No confident GitHub match: offer the weaker matches and the Overleaf login route.
function renderSourceCandidates(cands) {
  const box = $('#ol-cands'); box.hidden = false;
  box.innerHTML = `<div><b>${esc(N.olSel.title)}</b>: ${cands.length ? 'possible source repositories on GitHub (pick one to import):' : 'no source repository found on GitHub.'}</div>` +
    cands.map((c, i) => `<button class="cand" data-i="${i}">github.com/${esc(c.repo)}${c.stars ? ` ★ ${c.stars}` : ''}<span class="d">${esc(c.description)}</span></button>`).join('') +
    `<button class="cand alt" id="ol-via-overleaf">Open in Overleaf instead (needs your Overleaf login; then paste the project link below)</button>`;
  N.olCands = cands; dmsg('');
}
$('#ol-cands').addEventListener('click', async e => {
  const b = e.target.closest('button'); if (!b) return;
  if (b.id === 'ol-via-overleaf') { N.olSkipGithub = true; createProject(); N.olSkipGithub = false; return; }
  const c = N.olCands[+b.dataset.i]; if (!c) return;
  const name = $('#n-name').value.trim() || undefined; const remote = $('#n-remote').value.trim() || undefined;
  $('#n-create').disabled = true; dmsg(`Importing from github.com/${c.repo}…`);
  try { const r = await api('/api/projects/import-url', { method: 'POST', body: JSON.stringify({ name, url: c.url, remote }) }); if (r.main) preferences.setItem('gitleaf.main.' + r.name, r.main); dmsg(''); $('#ndlg').close(); toast(`Imported from github.com/${c.repo}`, 'ok', 5000); await loadProjects(r.name); }
  catch (err) { dmsg(err.message, true); }
  finally { $('#n-create').disabled = false; }
});
// After "Open as Template" in Overleaf, the project address is usually on the clipboard when the user comes back.
window.addEventListener('focus', async () => {
  if (!$('#ndlg').open || N.tab !== 'overleaf' || OL_PROJECT.test($('#ol-link').value) || !navigator.clipboard || !navigator.clipboard.readText) return;
  try { const t = await navigator.clipboard.readText(); const m = t && t.match(OL_PROJECT); if (m) { $('#ol-link').value = m[0]; olSteps(); dmsg('Project link taken from the clipboard. Press Import.'); $('#n-create').focus(); } } catch (e) { /* clipboard permission not granted */ }
});
function wireDrop(dropId, inputId, key) {
  const drop = $(dropId), input = $(inputId);
  const take = f => { if (!f) return; N[key] = f; drop.classList.add('has'); drop.firstChild.textContent = f.name; if (!$('#n-name').value) $('#n-name').value = guessName(f.name); };
  drop.addEventListener('click', () => input.click());
  input.addEventListener('change', () => take(input.files[0]));
  ['dragenter', 'dragover'].forEach(ev => drop.addEventListener(ev, e => { e.preventDefault(); drop.classList.add('over'); }));
  ['dragleave', 'drop'].forEach(ev => drop.addEventListener(ev, e => { e.preventDefault(); drop.classList.remove('over'); }));
  drop.addEventListener('drop', e => take(e.dataTransfer.files[0]));
}
wireDrop('#n-drop', '#n-zip', 'zipFile'); wireDrop('#ol-drop', '#ol-zip', 'olZip');
async function uploadZip(file, name, remote) {
  const resp = await fetch(`/api/projects/import-zip?name=${enc(name)}${remote ? '&remote=' + enc(remote) : ''}`, { method: 'POST', body: file });
  const r = await resp.json().catch(() => ({})); if (!resp.ok) throw new Error(r.error || resp.statusText); return r;
}
function dmsg(text, err) { const m = $('#n-msg'); m.textContent = text || ''; m.classList.toggle('err', !!err); }
async function createProject() {
  let name = $('#n-name').value.trim();
  const remote = $('#n-remote').value.trim() || undefined;
  const btn = $('#n-create'); btn.disabled = true; setStatus('creating…');
  dmsg(N.tab === 'git' || (N.tab === 'overleaf' && $('#ol-link').value.trim()) ? 'Cloning… (large projects take a while)' : N.tab === 'zip' && $('#n-url').value.trim() ? 'Downloading…' : 'Working…');
  try {
    let r;
    if (N.tab === 'templates') {
      if (!name) throw new Error('project name required');
      r = await api('/api/projects/new', { method: 'POST', body: JSON.stringify({ name, template: N.template, remote }) });
    } else if (N.tab === 'git') {
      const url = $('#n-git').value.trim(); if (!url) throw new Error('repository URL required');
      r = await api('/api/projects/clone', { method: 'POST', body: JSON.stringify({ url, name: name || undefined }) });
    } else if (N.tab === 'overleaf') {
      const link = $('#ol-link').value.trim();
      if (link) { r = await api('/api/projects/clone', { method: 'POST', body: JSON.stringify({ url: link, name: name || undefined }) }); }
      else if (N.olZip) { name = name || guessName(N.olZip.name); r = await uploadZip(N.olZip, name, remote); }
      else if (N.olSel && !N.olSkipGithub) {
        // One click: find the template's source repository on GitHub and import it from there.
        dmsg('Looking for this template\'s source on GitHub…');
        const cands = await api(`/api/overleaf/source?url=${enc(N.olSel.url)}&title=${enc(N.olSel.title)}`);
        const best = cands[0];
        if (best && (best.source === 'page' || best.stars >= 50)) {
          dmsg(`Importing from github.com/${best.repo}${best.source === 'page' ? ' (linked from the template page)' : ` (★ ${best.stars})`}…`);
          r = await api('/api/projects/import-url', { method: 'POST', body: JSON.stringify({ name: name || undefined, url: best.url, remote }) });
          toast(`Imported ${N.olSel.title} from github.com/${best.repo}`, 'ok', 5000);
        } else {
          renderSourceCandidates(cands); btn.disabled = false; return;
        }
      }
      else if (N.olSel) { window.open(N.olSel.url, '_blank', 'noopener'); dmsg('In the Overleaf tab press "Open as Template", then copy the project address and come back here.'); olSteps(); $('#ol-link').focus(); btn.disabled = false; return; }
      else throw new Error('Pick a template above (or paste an Overleaf project link, or drop its source zip).');
    } else {
      const url = $('#n-url').value.trim();
      if (url) r = await api('/api/projects/import-url', { method: 'POST', body: JSON.stringify({ name: name || undefined, url, remote }) });
      else if (N.zipFile) { name = name || guessName(N.zipFile.name); if (!name) throw new Error('project name required'); r = await uploadZip(N.zipFile, name, remote); }
      else throw new Error('give a zip URL or choose a zip file');
    }
    if (r.main) preferences.setItem('gitleaf.main.' + r.name, r.main);
    dmsg(''); $('#ndlg').close(); setStatus(`created ${r.name}`, 'ok');
    await loadProjects(r.name);
  } catch (e) { dmsg(e.message, true); setStatus(e.message.split('\n')[0], 'err'); setLog(e.message, true); }
  finally { btn.disabled = false; }
}
document.querySelectorAll('#ndlg .tabs button').forEach(b => b.onclick = () => showTab(b.dataset.tab));
$('#n-cards').addEventListener('click', e => { const c = e.target.closest('.card'); if (!c) return; N.template = c.dataset.id; document.querySelectorAll('#n-cards .card').forEach(x => x.classList.toggle('on', x === c)); });
$('#ol-tags').addEventListener('click', e => { const b = e.target.closest('button'); if (b) browseGallery(b.dataset.tag); });
$('#ol-go').onclick = () => browseGallery($('#ol-q').value);
$('#ol-q').addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); browseGallery($('#ol-q').value); } });
['#n-git', '#n-url', '#ol-link'].forEach(x => $(x).addEventListener('input', e => { if (!$('#n-name').value) $('#n-name').placeholder = guessName(e.target.value) || 'my-paper'; }));
$('#ndlg').addEventListener('keydown', e => { if (e.key === 'Enter' && e.target.tagName === 'INPUT' && e.target.id !== 'ol-q') { e.preventDefault(); createProject(); } });

// =====================================================================
// Visual (WYSIWYG) mode. The LaTeX source is split into blocks; prose blocks become
// editable rich text, everything else is kept verbatim. Only blocks the user touched are
// re-generated, so untouched LaTeX round-trips byte for byte.
// =====================================================================
const BLOCK_CMDS = new Set(['documentclass','usepackage','maketitle','tableofcontents','listoffigures','listoftables','newpage','clearpage','cleardoublepage','pagebreak','bibliography','bibliographystyle','printbibliography','input','include','includegraphics','centering','vspace','hspace','vfill','appendix','date','label','caption','setlength','newcommand','renewcommand','providecommand','def','let','item','hline','toprule','midrule','bottomrule','footnotesize','small','large','Large','LARGE','normalsize','acks','keywords','balance','received','ccsdesc','affiliation','email','orcid','settopmatter','copyrightyear','acmYear','acmConference','acmBooktitle','acmPrice','acmDOI','acmISBN','newcolumntype','graphicspath','begin','end','titlerunning','authorrunning','institute','subtitle','institute','usetheme','usecolortheme','frametitle','onehalfspacing','doublespacing','singlespacing','noindent','bigskip','medskip','smallskip','thispagestyle','pagestyle','setcounter','addcontentsline','titlepage','abstract','IEEEoverridecommandlockouts','titlerunning','subtitle','makeatletter','makeatother','newtheorem','theoremstyle','DeclareMathOperator','bibitem','onecolumn','twocolumn','frontmatter','mainmatter','backmatter','chapter*','section*']);
const HEAD_RE = /^(\s*)(\\(part|chapter|section|subsection|subsubsection|paragraph|subparagraph|title|author)(\*?)(\[[^\]]*\])?)\{/;
const MATH_ENVS = new Set(['equation','equation*','align','align*','gather','gather*','multline','multline*','eqnarray','eqnarray*','displaymath','alignat','alignat*','flalign','flalign*']);
const TABLE_ENVS = new Set(['table','table*','tabular','tabularx','tabular*','longtable','threeparttable','sidewaystable']);
const FIGURE_ENVS = new Set(['figure','figure*','wrapfigure','sidewaysfigure']);
const CONTAINER_ENVS = new Set(['abstract','quote','quotation','center','flushleft','flushright','small','footnotesize','frame','minipage','multicols','acks']);
const LIST_ENVS = new Set(['itemize','enumerate','description']);
const LEVEL_TAG = { part: 'h1', chapter: 'h1', section: 'h2', subsection: 'h3', subsubsection: 'h4', paragraph: 'h5', subparagraph: 'h5', title: 'h1', author: 'div' };
const STYLE_CMDS = { textbf: 'b', emph: 'i', textit: 'i', underline: 'u' };

function parseDoc(src) {
  const bi = src.indexOf('\\begin{document}');
  if (bi < 0) return { pre: '', body: src, post: '' };
  const bs = bi + '\\begin{document}'.length;
  const ei = src.lastIndexOf('\\end{document}');
  return { pre: src.slice(0, bs), body: src.slice(bs, ei < 0 ? src.length : ei), post: ei < 0 ? '' : src.slice(ei) };
}
function matchBrace(s, from) { let d = 0; for (let k = from; k < s.length; k++) { const c = s[k]; if (c === '\\') { k++; continue; } if (c === '{') d++; else if (c === '}') { d--; if (d === 0) return k; } } return -1; }
function envName(line) { const m = /\\begin\{([A-Za-z*]+)\}/.exec(line); return m ? m[1] : null; }
function findEnvEnd(lines, i, name) {
  const b = new RegExp('\\\\begin\\{' + name.replace('*', '\\*') + '\\}', 'g'), e = new RegExp('\\\\end\\{' + name.replace('*', '\\*') + '\\}', 'g');
  let depth = 0;
  for (let j = i; j < lines.length; j++) {
    depth += (lines[j].match(b) || []).length; depth -= (lines[j].match(e) || []).length;
    if (depth <= 0) return j;
  }
  return -1;
}
function blockCmd(line) { const m = /^\s*\\([A-Za-z]+\*?)/.exec(line); return m && BLOCK_CMDS.has(m[1]) ? m[1] : null; }
function isBoundary(line) { return line.trim() === '' || !!envName(line) || HEAD_RE.test(line) || !!blockCmd(line) || /^\s*\\end\{/.test(line) || /^\s*\\\[/.test(line) || /^\s*\$\$/.test(line); }

function parseBlocks(text) {
  const lines = text.split('\n');
  const last = lines.length - 1;
  const L = (a, b) => lines.slice(a, b + 1).map((l, k) => l + (a + k < last ? '\n' : '')).join('');
  const blocks = []; let lead = ''; let i = 0;
  const addSep = s => { if (blocks.length) blocks[blocks.length - 1].sep += s; else lead += s; };
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === '') { if (i < last || line !== '') addSep(L(i, i)); i++; continue; }
    if (/^\s*%/.test(line)) { let j = i; while (j + 1 <= last && /^\s*%/.test(lines[j + 1])) j++; blocks.push({ kind: 'raw', cls: 'comment', raw: L(i, j), sep: '' }); i = j + 1; continue; }
    const env = envName(line);
    if (env) {
      const j = findEnvEnd(lines, i, env);
      if (j < 0) { blocks.push({ kind: 'raw', raw: L(i, last), sep: '' }); break; }
      const openOk = new RegExp('^\\s*\\\\begin\\{' + env.replace('*', '\\*') + '\\}(\\[[^\\]]*\\]|\\{[^}]*\\})*\\s*$').test(line);
      const closeOk = new RegExp('^\\s*\\\\end\\{' + env.replace('*', '\\*') + '\\}\\s*$').test(lines[j]);
      if (openOk && closeOk && j > i && LIST_ENVS.has(env)) {
        const inner = lines.slice(i + 1, j).join('\n');
        if (!/\\begin\{/.test(inner) && /^\s*\\item(\s|$)/m.test(inner) && !/\\item\[/.test(inner)) {
          const parts = inner.split(/^\s*\\item\s?/m);
          if (parts[0].trim() === '') {
            blocks.push({ kind: 'list', env, open: lines[i].trim(), close: lines[j].trim(), items: parts.slice(1).map(p => p.replace(/\s+$/, '')), raw: L(i, j), sep: '' });
            i = j + 1; continue;
          }
        }
      }
      if (openOk && closeOk && j > i + 1 && CONTAINER_ENVS.has(env)) {
        blocks.push({ kind: 'container', env, open: lines[i].trim(), close: lines[j].trim(), inner: parseBlocks(lines.slice(i + 1, j).join('\n') + '\n'), raw: L(i, j), sep: '' });
        i = j + 1; continue;
      }
      const base = env.replace('*', '');
      const kind = MATH_ENVS.has(env) ? 'math' : (TABLE_ENVS.has(env) || TABLE_ENVS.has(base)) ? 'table' : (FIGURE_ENVS.has(env) || FIGURE_ENVS.has(base)) ? 'figure' : 'raw';
      blocks.push({ kind, raw: L(i, j), sep: '' }); i = j + 1; continue;
    }
    if (/^\s*\\\[/.test(line) || /^\s*\$\$/.test(line)) {
      const closer = /^\s*\\\[/.test(line) ? '\\]' : '$$';
      let j = i; const startAt = line.indexOf(closer === '\\]' ? '\\[' : '$$') + 2;
      while (j <= last && (j === i ? lines[j].indexOf(closer, startAt) < 0 : !lines[j].includes(closer))) j++;
      if (j > last) j = last;
      blocks.push({ kind: 'math', raw: L(i, j), sep: '' }); i = j + 1; continue;
    }
    const hm = HEAD_RE.exec(line);
    if (hm) {
      const bracePos = hm[0].length - 1; const e = matchBrace(line, bracePos);
      if (e > 0) { blocks.push({ kind: 'heading', level: hm[3], prefix: hm[1] + hm[2], inner: line.slice(bracePos + 1, e), trailing: line.slice(e + 1), raw: L(i, i), sep: '' }); i++; continue; }
    }
    if (blockCmd(line) || /^\s*\\end\{/.test(line)) { blocks.push({ kind: 'raw', raw: L(i, i), sep: '' }); i++; continue; }
    let j = i; while (j + 1 <= last && !isBoundary(lines[j + 1])) j++;
    blocks.push({ kind: 'para', raw: L(i, j), sep: '' }); i = j + 1;
  }
  return { lead, blocks };
}

function atomLabel(name, raw) {
  const arg = () => { const m = /\{([^{}]*)\}\s*$/.exec(raw); return m ? m[1] : ''; };
  if (/^cite|^parencite|^textcite|^autocite|^citep|^citet|^citeauthor|^citeyear/.test(name)) return '[' + arg() + ']';
  if (/^(ref|cref|Cref|autoref|eqref|pageref|nameref)$/.test(name)) return '→' + arg();
  if (name === 'label') return '#' + arg();
  if (name === 'footnote') return 'fn: ' + arg().slice(0, 30) + (arg().length > 30 ? '…' : '');
  if (name === 'url' || name === 'href') return 'link: ' + arg().slice(0, 30);
  if (name === 'includegraphics') return 'image: ' + arg();
  return raw.length <= 24 ? raw : '\\' + name + '{…}';
}
function parseInline(s) {
  const nodes = []; let i = 0, text = '';
  const flush = () => { if (text) { nodes.push({ t: 'text', v: text }); text = ''; } };
  while (i < s.length) {
    const c = s[i];
    if (c === '\\') {
      const n = s[i + 1];
      if (n === '\\') { flush(); let j = i + 2; if (s[j] === '*') j++; if (s[j] === '[') { const e = s.indexOf(']', j); if (e > 0) j = e + 1; } nodes.push({ t: 'br', raw: s.slice(i, j) }); i = j; continue; }
      if (n === '(' || n === '[') { const close = n === '(' ? '\\)' : '\\]'; const e = s.indexOf(close, i + 2); const j = e < 0 ? s.length : e + 2; flush(); nodes.push({ t: 'math', raw: s.slice(i, j), display: n === '[' }); i = j; continue; }
      if (n !== undefined && /[A-Za-z]/.test(n)) {
        let j = i + 1; while (j < s.length && /[A-Za-z]/.test(s[j])) j++;
        const name = s.slice(i + 1, j); if (s[j] === '*') j++;
        const style = STYLE_CMDS[name];
        if (style && s[j] === '{') { const e = matchBrace(s, j); if (e > 0) { flush(); nodes.push({ t: style, children: parseInline(s.slice(j + 1, e)) }); i = e + 1; continue; } }
        let k = j;
        while (k < s.length && (s[k] === '[' || s[k] === '{')) { if (s[k] === '[') { const e = s.indexOf(']', k); if (e < 0) break; k = e + 1; } else { const e = matchBrace(s, k); if (e < 0) break; k = e + 1; } }
        flush(); nodes.push({ t: 'atom', raw: s.slice(i, k), label: atomLabel(name, s.slice(i, k)) }); i = k; continue;
      }
      if (n !== undefined && '%&_$#{}'.includes(n)) { text += n; i += 2; continue; }
      flush(); nodes.push({ t: 'atom', raw: s.slice(i, i + 2), label: s.slice(i, i + 2) }); i += 2; continue;
    }
    if (c === '$') { const dbl = s[i + 1] === '$'; const open = dbl ? '$$' : '$'; const e = s.indexOf(open, i + open.length); const j = e < 0 ? s.length : e + open.length; flush(); nodes.push({ t: 'math', raw: s.slice(i, j), display: dbl }); i = j; continue; }
    if (c === '{') { const e = matchBrace(s, i); const j = e < 0 ? s.length : e + 1; flush(); nodes.push({ t: 'atom', raw: s.slice(i, j), label: s.slice(i, j).length <= 24 ? s.slice(i, j) : '{…}' }); i = j; continue; }
    if (c === '%') { let e = s.indexOf('\n', i); if (e < 0) e = s.length; flush(); nodes.push({ t: 'atom', raw: s.slice(i, e), label: s.slice(i, e).slice(0, 40), cls: 'comment' }); i = e; continue; }
    if (c === '~') { text += '\u00a0'; i++; continue; }
    text += c; i++;
  }
  flush(); return nodes;
}
function mathEl(raw, display) {
  const el = document.createElement('span'); el.className = 'atom math' + (display ? ' display' : ''); el.contentEditable = 'false'; el.dataset.raw = raw; el.title = raw;
  let inner = raw;
  if (raw.startsWith('$$')) inner = raw.slice(2, -2); else if (raw.startsWith('$')) inner = raw.slice(1, -1); else inner = raw.slice(2, -2);
  if (window.katex) { try { katex.render(inner, el, { displayMode: !!display, throwOnError: false }); } catch (e) { el.textContent = raw; } } else el.textContent = raw;
  return el;
}
function atomEl(raw, label, cls) { const el = document.createElement('span'); el.className = 'atom' + (cls ? ' ' + cls : ''); el.contentEditable = 'false'; el.dataset.raw = raw; el.title = raw; el.textContent = label; return el; }
function renderInline(nodes) {
  return nodes.map(n => {
    if (n.t === 'text') return document.createTextNode(n.v);
    if (n.t === 'br') { const br = document.createElement('br'); br.dataset.raw = n.raw; return br; }
    if (n.t === 'math') return mathEl(n.raw, n.display);
    if (n.t === 'atom') return atomEl(n.raw, n.label, n.cls);
    const el = document.createElement(n.t); el.append(...renderInline(n.children)); return el;
  });
}
function renderBlock(b) {
  let el;
  if (b.kind === 'para') { el = document.createElement('p'); el.contentEditable = 'true'; el.append(...renderInline(parseInline(b.raw.replace(/\n$/, '')))); }
  else if (b.kind === 'heading') { el = document.createElement(LEVEL_TAG[b.level] || 'h3'); el.contentEditable = 'true'; el.dataset.prefix = b.prefix; el.dataset.trailing = b.trailing; el.dataset.level = b.level; if (b.level === 'title' || b.level === 'author') el.classList.add(b.level); el.append(...renderInline(parseInline(b.inner))); }
  else if (b.kind === 'math' || b.kind === 'table' || b.kind === 'figure') { el = document.createElement('div'); el.className = 'fancy ' + b.kind; el._src = b.raw.replace(/\n$/, ''); renderFancy(el); }
  else if (b.kind === 'list') {
    el = document.createElement(b.env === 'enumerate' ? 'ol' : 'ul'); el.contentEditable = 'true'; el.dataset.open = b.open; el.dataset.close = b.close;
    for (const it of b.items) { const li = document.createElement('li'); li.append(...renderInline(parseInline(it))); el.appendChild(li); }
  }
  else if (b.kind === 'container') {
    el = document.createElement('div'); el.className = 'container'; el.dataset.open = b.open; el.dataset.close = b.close;
    const o = document.createElement('span'); o.className = 'chip'; o.textContent = b.open; o.contentEditable = 'false';
    const inner = document.createElement('div'); inner.className = 'inner'; renderBlocks(inner, b.inner);
    const c = document.createElement('span'); c.className = 'chip'; c.textContent = b.close; c.contentEditable = 'false';
    el.append(o, inner, c);
  }
  else { el = document.createElement('pre'); el.className = 'raw' + (b.cls ? ' ' + b.cls : ''); el.contentEditable = 'plaintext-only'; el.textContent = b.raw.replace(/\n$/, ''); }
  el.classList.add('blk'); el.dataset.kind = b.kind; el._raw = b.raw; el._sep = b.sep; el._nl = b.raw.endsWith('\n');
  return el;
}
// ---- rendered blocks (math, table, figure): shown rendered, double-click to edit the LaTeX,
// captions editable in place. `el._src` holds the current LaTeX of the block.
function fancySource(el) {
  const cap = el.querySelector('.cap-text');
  if (cap && el._capRange && cap.dataset.edited) {
    const [a, b] = el._capRange;
    const src = el._src.slice(0, a) + serInline(cap).trim() + el._src.slice(b);
    el._src = src; el._capRange = [a, a + serInline(cap).trim().length]; cap.dataset.edited = '';
  }
  return el._src;
}
function findCaption(src) {
  const i = src.indexOf('\\caption');
  if (i < 0) return null;
  let j = i + 8; if (src[j] === '[') { j = src.indexOf(']', j) + 1; if (j <= 0) return null; }
  while (src[j] === ' ') j++;
  if (src[j] !== '{') return null;
  const e = matchBrace(src, j); if (e < 0) return null;
  return [j + 1, e];
}
function metaChips(src, skip) {
  const chips = []; const first = src.split('\n')[0].trim();
  if (first) chips.push(atomEl(first, first.length > 40 ? first.slice(0, 40) + '…' : first));
  const labels = src.match(/\\label\{[^}]*\}/g) || [];
  for (const l of labels) chips.push(atomEl(l, '#' + l.slice(7, -1)));
  const m = document.createElement('div'); m.className = 'meta'; m.contentEditable = 'false'; m.append(...chips); return m;
}
function captionEl(el, src, label) {
  const r = findCaption(src); if (!r) { el._capRange = null; return null; }
  el._capRange = r;
  const c = document.createElement('div'); c.className = 'caption';
  const lab = document.createElement('span'); lab.className = 'cap-label'; lab.textContent = label + ':'; lab.contentEditable = 'false';
  const t = document.createElement('span'); t.className = 'cap-text'; t.contentEditable = 'true'; t.append(...renderInline(parseInline(src.slice(r[0], r[1]))));
  t.addEventListener('input', () => { t.dataset.edited = '1'; });
  c.append(lab, t); return c;
}
function splitTopLevel(text, sep) {
  const parts = []; let cur = '', depth = 0;
  for (let k = 0; k < text.length; k++) {
    const ch = text[k];
    if (ch === '\\') { if (sep === '\\\\' && text[k + 1] === '\\' && depth === 0) { parts.push(cur); cur = ''; k++; continue; } cur += ch + (text[k + 1] ?? ''); k++; continue; }
    if (ch === '{') depth++; else if (ch === '}') depth--;
    if (depth === 0 && ch === sep && sep !== '\\\\') { parts.push(cur); cur = ''; continue; }
    cur += ch;
  }
  parts.push(cur); return parts;
}
function renderTable(el, src) {
  const tb = src.indexOf('\\begin{tabular'); const te = src.lastIndexOf('\\end{tabular');
  if (tb < 0 || te < tb) return false;
  let i = src.indexOf('}', tb) + 1;
  while (src[i] === '[' || src[i] === '{') { const e = src[i] === '[' ? src.indexOf(']', i) : matchBrace(src, i); if (e < 0) return false; i = e + 1; }
  const body = src.slice(i, te);
  const rawRows = splitTopLevel(body, '\\\\');
  const rows = []; let ruleNext = false, sawMid = false, headerRows = 0;
  for (let r of rawRows) {
    const hadTop = /\\(toprule|hline)/.test(r);
    const hadMid = /\\(midrule|cmidrule|cline)/.test(r);
    r = r.replace(/\\(toprule|midrule|bottomrule|hline)\b/g, '').replace(/\\c(midrule|line)(\([^)]*\))?\{[^}]*\}/g, '').replace(/\[[^\]]*\]\s*$/, '');
    if (hadMid && rows.length && !sawMid) { sawMid = true; headerRows = rows.length; }
    if (r.trim() === '') { if (hadTop || hadMid) ruleNext = true; continue; }
    rows.push({ cells: splitTopLevel(r, '&').map(c => c.trim()), rule: ruleNext || hadTop }); ruleNext = hadMid && !sawMid ? false : false;
  }
  if (!rows.length) return false;
  const table = document.createElement('table'); table.className = 'tbl'; table.contentEditable = 'false';
  rows.forEach((row, ri) => {
    const tr = document.createElement('tr'); if (row.rule && ri > 0) tr.className = 'rule-above';
    for (const c of row.cells) { const td = document.createElement(ri < headerRows ? 'th' : 'td'); td.append(...renderInline(parseInline(c))); tr.appendChild(td); }
    table.appendChild(tr);
  });
  el.appendChild(table); return true;
}
function imageUrl(path) {
  const clean = path.trim().replace(/^"|"$/g, '');
  return `${P()}/raw?path=${enc(clean)}`;
}
async function resolveImage(holder, path) {
  const hasExt = /\.[A-Za-z0-9]{2,4}$/.test(path);
  const candidates = hasExt ? [path] : ['png', 'jpg', 'jpeg', 'pdf', 'svg', 'gif'].map(e => path + '.' + e);
  for (const c of candidates) {
    try { const r = await fetch(imageUrl(c), { method: 'HEAD' }); if (!r.ok) continue; } catch (e) { continue; }
    holder.innerHTML = '';
    if (/\.pdf$/i.test(c)) { const o = document.createElement('object'); o.data = imageUrl(c); o.type = 'application/pdf'; o.textContent = c; holder.appendChild(o); }
    else { const img = document.createElement('img'); img.src = imageUrl(c); img.alt = c; holder.appendChild(img); }
    return;
  }
  holder.innerHTML = ''; const m = document.createElement('span'); m.className = 'missing'; m.textContent = 'image not found: ' + path; holder.appendChild(m);
}
function renderFigure(el, src) {
  const fig = document.createElement('div'); fig.className = 'fig'; fig.contentEditable = 'false';
  const re = /\\includegraphics\*?(?:\[[^\]]*\])?\{([^}]*)\}/g; let m, n = 0;
  while ((m = re.exec(src))) { n++; const holder = document.createElement('span'); holder.className = 'missing'; holder.textContent = 'loading ' + m[1]; fig.appendChild(holder); resolveImage(holder, m[1]); }
  if (!n) {
    const tikz = /\\begin\{(tikzpicture|axis|pgfpicture)\}/.test(src);
    const t = document.createElement('details'); t.className = 'missing';
    const sum = document.createElement('summary'); sum.textContent = tikz ? 'TikZ / pgfplots drawing (rendered in the PDF). Click to view source.' : 'figure without \\includegraphics. Click to view source.';
    const pre = document.createElement('pre'); pre.className = 'raw'; pre.textContent = src; t.append(sum, pre); fig.appendChild(t);
  }
  el.appendChild(fig); return true;
}
function renderMath(el, src) {
  let tex = src.trim();
  const labels = tex.match(/\\label\{[^}]*\}/g) || [];
  tex = tex.replace(/\\label\{[^}]*\}/g, '');
  if (tex.startsWith('\\[')) tex = tex.slice(2, -2); else if (tex.startsWith('$$')) tex = tex.slice(2, -2);
  else if (/^\\begin\{(displaymath|equation\*?)\}/.test(tex)) tex = tex.replace(/^\\begin\{[^}]*\}/, '').replace(/\\end\{[^}]*\}\s*$/, '');
  const box = document.createElement('div'); box.className = 'mathblk'; box.contentEditable = 'false';
  if (window.katex) { try { katex.render(tex, box, { displayMode: true, throwOnError: false }); } catch (e) { box.textContent = src; } } else box.textContent = src;
  if (labels.length) { const l = document.createElement('div'); l.className = 'labels'; l.append(...labels.map(x => atomEl(x, '#' + x.slice(7, -1)))); box.appendChild(l); }
  el.appendChild(box); return true;
}
function renderFancy(el) {
  const src = el._src; el.innerHTML = ''; el.classList.remove('editing');
  const kind = el.classList.contains('math') ? 'math' : el.classList.contains('table') ? 'table' : 'figure';
  let ok = false;
  if (kind === 'math') ok = renderMath(el, src);
  else { el.appendChild(metaChips(src)); ok = kind === 'table' ? renderTable(el, src) : renderFigure(el, src); const cap = captionEl(el, src, kind === 'table' ? 'Table' : 'Figure'); if (cap) el.appendChild(cap); }
  if (!ok) { const pre = document.createElement('pre'); pre.className = 'raw'; pre.textContent = src; el.appendChild(pre); }
}
function editFancy(el) {
  if (el.classList.contains('editing')) return;
  const src = fancySource(el); el.innerHTML = ''; el.classList.add('editing');
  const pre = document.createElement('pre'); pre.className = 'src'; pre.contentEditable = 'plaintext-only'; pre.textContent = src;
  pre.addEventListener('blur', () => { const v = pre.textContent; if (v !== el._src) { el._src = v; markEdited(el); } renderFancy(el); });
  pre.addEventListener('keydown', e => { if (e.key === 'Escape') pre.blur(); });
  el.appendChild(pre); placeCaret(pre, false);
}
function renderBlocks(container, parsed) { container._lead = parsed.lead; for (const b of parsed.blocks) container.appendChild(renderBlock(b)); }

function escapeTex(t) {
  return t.replace(/\\/g, '\\textbackslash{}').replace(/([{}$&#%_])/g, '\\$1').replace(/\^/g, '\\^{}').replace(/~/g, '\\textasciitilde{}').replace(/\u00a0/g, '~');
}
function serInline(el) {
  let out = '';
  for (const n of el.childNodes) {
    if (n.nodeType === 3) { out += escapeTex(n.nodeValue); continue; }
    if (n.nodeType !== 1) continue;
    if (n.dataset && n.dataset.raw !== undefined) { out += n.dataset.raw; continue; }
    const tag = n.tagName, st = n.style || {};
    if (tag === 'BR') { out += '\\\\\n'; continue; }
    const inner = serInline(n);
    if (tag === 'B' || tag === 'STRONG' || st.fontWeight === 'bold' || +st.fontWeight >= 600) out += '\\textbf{' + inner + '}';
    else if (tag === 'I' || tag === 'EM' || st.fontStyle === 'italic') out += '\\emph{' + inner + '}';
    else if (tag === 'U' || (st.textDecoration || '').includes('underline')) out += '\\underline{' + inner + '}';
    else if (tag === 'DIV' || tag === 'P') out += '\n\n' + inner;
    else if (tag === 'LI') out += '\n\\item ' + inner;
    else out += inner;
  }
  return out;
}
function serializeBlock(el) {
  const nl = el._nl === undefined ? true : el._nl;
  if (!el.dataset.edited && el._raw !== undefined) return el._raw;
  const k = el.dataset.kind; let s;
  if (k === 'para') s = serInline(el).replace(/^\n+/, '').replace(/\n{3,}/g, '\n\n');
  else if (k === 'heading') s = el.dataset.prefix + '{' + serInline(el).trim() + '}' + (el.dataset.trailing || '');
  else if (k === 'list') s = el.dataset.open + '\n' + [...el.children].filter(li => li.tagName === 'LI').map(li => '  \\item ' + serInline(li).trim()).join('\n') + '\n' + el.dataset.close;
  else if (k === 'container') s = el.dataset.open + '\n' + serializeBlocks(el.querySelector(':scope > .inner')) + el.dataset.close;
  else if (k === 'math' || k === 'table' || k === 'figure') s = fancySource(el);
  else s = el.textContent;
  return s + (nl ? '\n' : '');
}
function serializeBlocks(container) {
  let out = container._lead || '';
  for (const el of container.children) { if (!el.classList.contains('blk')) continue; out += serializeBlock(el) + (el._sep ?? '\n'); }
  return out;
}
function serializeDoc() {
  const v = $('#visual');
  const pre = v.querySelector('.pre-raw'), post = v.querySelector('.post-raw');
  return (pre ? pre.textContent : '') + serializeBlocks(v.querySelector('.body')) + (post ? post.textContent : '');
}

function renderVisual() {
  const v = $('#visual'); v.innerHTML = '';
  const d = parseDoc(cm.getValue());
  if (d.pre) { const det = document.createElement('details'); det.innerHTML = `<summary>Preamble (${d.pre.split('\n').length} lines) — click to edit</summary>`; const pre = document.createElement('pre'); pre.className = 'raw pre-raw'; pre.contentEditable = 'plaintext-only'; pre.textContent = d.pre; det.appendChild(pre); v.appendChild(det); }
  const body = document.createElement('div'); body.className = 'body'; renderBlocks(body, parseBlocks(d.body)); v.appendChild(body);
  if (d.post) { const det = document.createElement('details'); det.innerHTML = `<summary>End of document (${d.post.split('\n').length} lines)</summary>`; const pre = document.createElement('pre'); pre.className = 'raw post-raw'; pre.contentEditable = 'plaintext-only'; pre.textContent = d.post; det.appendChild(pre); v.appendChild(det); }
  if (!body.children.length) { const p = newBlock('para'); body.appendChild(p); }
}
function syncFromVisual() {
  if (!state.visual) return;
  const src = serializeDoc();
  if (src !== cm.getValue()) { state.loading = true; cm.setValue(src); state.loading = false; }
}
async function enterVisual() {
  if (!state.file || !/\.tex$/i.test(state.file)) { setStatus('Visual mode works on .tex files', 'err'); return; }
  try { await ensureKatex(); } catch (e) { setStatus('KaTeX not loaded; math shows as source', 'err'); }
  state.visual = true; $('#editor').classList.add('visual'); $('#modebtn').textContent = 'Code'; $('#modebtn').classList.add('on');
  renderVisual();
}
function exitVisual() {
  syncFromVisual(); state.visual = false; $('#editor').classList.remove('visual'); $('#modebtn').textContent = 'Visual'; $('#modebtn').classList.remove('on');
  setTimeout(() => { cm.refresh(); cm.focus(); }, 0);
}
function markEdited(el) { for (let b = el; b; b = b.parentElement) { if (b.classList && b.classList.contains('blk')) b.dataset.edited = '1'; if (b.id === 'visual') break; } state.dirty = true; markDirty(); scheduleSave(); }
function currentBlock() { const sel = getSelection(); let n = sel.anchorNode; if (!n) return null; if (n.nodeType === 3) n = n.parentElement; return n.closest ? n.closest('#visual .blk') : null; }
function newBlock(kind, env) {
  let el;
  if (kind === 'para') { el = document.createElement('p'); el.contentEditable = 'true'; el.innerHTML = '<br>'; }
  else if (kind === 'list') { el = document.createElement(env === 'enumerate' ? 'ol' : 'ul'); el.contentEditable = 'true'; el.dataset.open = `\\begin{${env}}`; el.dataset.close = `\\end{${env}}`; el.innerHTML = '<li><br></li>'; }
  else if (kind === 'raw') { el = document.createElement('pre'); el.className = 'raw'; el.contentEditable = 'plaintext-only'; el.textContent = env || ''; }
  else { el = document.createElement('div'); el.className = 'fancy ' + kind; el._src = env || ''; renderFancy(el); }
  el.classList.add('blk'); el.dataset.kind = kind; el.dataset.edited = '1'; el._sep = '\n\n'; el._nl = true;
  return el;
}
function placeCaret(el, atEnd) { const r = document.createRange(); r.selectNodeContents(el); r.collapse(!atEnd); const s = getSelection(); s.removeAllRanges(); s.addRange(r); el.focus(); }
function insertAfter(ref, el) { const body = $('#visual .body'); if (ref && ref.parentElement) ref.after(el); else body.appendChild(el); markEdited(el); placeCaret(el.querySelector('li') || el, false); }
function insertAtomAtCaret(raw) {
  const blk = currentBlock(); if (!blk || blk.dataset.kind === 'raw' || blk.dataset.kind === 'container') { setStatus('place the cursor in a paragraph first', 'err'); return; }
  const sel = getSelection(); const r = sel.getRangeAt(0); r.deleteContents();
  const nodes = renderInline(parseInline(raw)); const frag = document.createDocumentFragment(); nodes.forEach(n => frag.appendChild(n));
  const lastNode = nodes[nodes.length - 1]; r.insertNode(frag); r.setStartAfter(lastNode); r.collapse(true); sel.removeAllRanges(); sel.addRange(r);
  markEdited(blk);
}
function setBlockKind(blk, kind) {
  if (!blk || !['para', 'heading'].includes(blk.dataset.kind)) { setStatus('select a paragraph or heading first', 'err'); return; }
  let el;
  if (kind === 'para') { el = document.createElement('p'); }
  else { el = document.createElement(LEVEL_TAG[kind]); el.dataset.prefix = '\\' + kind; el.dataset.trailing = ''; el.dataset.level = kind; }
  el.contentEditable = 'true'; el.className = 'blk'; el.dataset.kind = kind === 'para' ? 'para' : 'heading'; el.dataset.edited = '1'; el._sep = blk._sep; el._nl = true;
  while (blk.firstChild) el.appendChild(blk.firstChild);
  blk.replaceWith(el); markEdited(el); placeCaret(el, true);
}
function splitBlock(blk) {
  const sel = getSelection(); const r = sel.getRangeAt(0);
  const tail = document.createRange(); tail.setStart(r.endContainer, r.endOffset); tail.setEndAfter(blk.lastChild || blk);
  const frag = blk.contains(tail.endContainer) ? tail.extractContents() : document.createDocumentFragment();
  const p = newBlock('para'); p.innerHTML = ''; p.appendChild(frag); if (!p.firstChild) p.innerHTML = '<br>';
  if (!blk.firstChild) blk.innerHTML = '<br>';
  p._sep = blk._sep; blk._sep = '\n\n'; markEdited(blk); blk.after(p); markEdited(p); placeCaret(p, false);
}
$('#visual').addEventListener('input', e => { const blk = e.target.closest ? e.target.closest('.blk') : null; if (blk) markEdited(blk); else if (e.target.classList && (e.target.classList.contains('pre-raw') || e.target.classList.contains('post-raw'))) { if (!state.dirty) { state.dirty = true; markDirty(); } } });
$('#visual').addEventListener('keydown', e => {
  const blk = currentBlock(); if (!blk) return;
  const k = blk.dataset.kind;
  if (e.target.closest && e.target.closest('.cap-text') && e.key === 'Enter') { e.preventDefault(); return; }
  if (e.key === 'Enter' && (k === 'para' || k === 'heading')) {
    e.preventDefault();
    if (e.shiftKey && k === 'para') { document.execCommand('insertHTML', false, '<br>'); markEdited(blk); return; }
    splitBlock(blk); return;
  }
  if (e.key === 'Backspace' && k === 'para' && blk.textContent === '' ) {
    const prev = blk.previousElementSibling; if (!prev) return; e.preventDefault(); blk.remove(); markEdited(prev);
    const target = prev.dataset.kind === 'list' ? prev.lastElementChild : prev; placeCaret(target, true); return;
  }
  if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); compile(); }
});
$('#visual').addEventListener('dblclick', e => {
  const fancy = e.target.closest('.fancy');
  if (fancy && !e.target.closest('.cap-text') && !fancy.classList.contains('editing')) { e.preventDefault(); editFancy(fancy); return; }
  const atom = e.target.closest('.atom'); if (!atom) return;
  const raw = prompt('Edit LaTeX:', atom.dataset.raw); if (raw === null || raw === atom.dataset.raw) return;
  const nodes = renderInline(parseInline(raw)); const blk = atom.closest('.blk');
  if (nodes.length) { atom.replaceWith(...nodes); } else atom.remove();
  if (blk) markEdited(blk);
});
$('#visual').addEventListener('paste', e => {
  const blk = currentBlock(); if (!blk || blk.dataset.kind === 'raw') return;
  e.preventDefault(); const text = (e.clipboardData || window.clipboardData).getData('text/plain');
  const nodes = renderInline(parseInline(text.replace(/\r\n?/g, '\n'))); const sel = getSelection(); const r = sel.getRangeAt(0); r.deleteContents();
  const frag = document.createDocumentFragment(); nodes.forEach(n => frag.appendChild(n)); const lastNode = nodes[nodes.length - 1]; r.insertNode(frag);
  if (lastNode) { r.setStartAfter(lastNode); r.collapse(true); sel.removeAllRanges(); sel.addRange(r); }
  markEdited(blk);
});
$('#vtoolbar').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  const blk = currentBlock();
  if (b.dataset.cmd) { document.execCommand(b.dataset.cmd); if (blk) markEdited(blk); return; }
  if (b.dataset.blk) { setBlockKind(blk, b.dataset.blk); return; }
  if (b.dataset.ins) {
    const t = b.dataset.ins;
    if (t === 'cite') { const key = prompt('Citation key(s), comma separated:'); if (key) insertAtomAtCaret(`\\cite{${key.trim()}}`); }
    else if (t === 'ref') { const l = prompt('Label to reference:'); if (l) insertAtomAtCaret(`\\ref{${l.trim()}}`); }
    else if (t === 'math') { const m = prompt('Inline math (without $):', 'x^2'); if (m) insertAtomAtCaret(`$${m}$`); }
    else if (t === 'footnote') { const f = prompt('Footnote text:'); if (f) insertAtomAtCaret(`\\footnote{${f}}`); }
    return;
  }
  if (b.dataset.new) {
    const t = b.dataset.new; let el;
    if (t === 'list') el = newBlock('list', 'itemize');
    else if (t === 'enum') el = newBlock('list', 'enumerate');
    else if (t === 'figure') el = newBlock('figure', '\\begin{figure}[t]\n  \\centering\n  \\includegraphics[width=\\linewidth]{figures/example.pdf}\n  \\caption{Caption.}\n  \\label{fig:example}\n\\end{figure}');
    else if (t === 'table') el = newBlock('table', '\\begin{table}[t]\n  \\centering\n  \\caption{Caption.}\n  \\label{tab:example}\n  \\begin{tabular}{lrr}\n    \\toprule\n    Item & A & B \\\\\n    \\midrule\n    One & 1 & 2 \\\\\n    \\bottomrule\n  \\end{tabular}\n\\end{table}');
    else if (t === 'equation') el = newBlock('math', '\\begin{equation}\n  y = f(x)\n  \\label{eq:example}\n\\end{equation}');
    else el = newBlock('raw', '');
    insertAfter(blk, el);
  }
});

// ------------------------------------------------------------------ resizable panes
(function () {
  const main = $('main');
  let saved = {};
  try { const value = JSON.parse(preferences.getItem('gitleaf.panes') || '{}'); if (value && typeof value === 'object') saved = value; } catch {}
  for (const key of ['sidew', 'pdfw']) if (!Number.isFinite(saved[key]) || saved[key] < 140 || saved[key] > innerWidth * .65) delete saved[key];
  if (saved.sidew) main.style.setProperty('--sidew', saved.sidew + 'px');
  if (saved.pdfw) main.style.setProperty('--pdfw', saved.pdfw + 'px');
  let drag = null;
  $$('.gutter').forEach(g => {
    g.addEventListener('keydown', e => {
      if (!['ArrowLeft', 'ArrowRight', 'Home'].includes(e.key)) return; e.preventDefault();
      if (e.key === 'Home') { g.dispatchEvent(new MouseEvent('dblclick')); return; }
      const key = g.dataset.for, direction = e.key === 'ArrowRight' ? 1 : -1;
      const width = key === 'sidew' ? $('#side').offsetWidth : $('#pdfpane').offsetWidth;
      const next = Math.max(140, Math.min(innerWidth * .65, width + direction * (key === 'pdfw' ? -20 : 20)));
      main.style.setProperty('--' + key, next + 'px'); saved[key] = next; preferences.setItem('gitleaf.panes', JSON.stringify(saved));
    });
    g.addEventListener('mousedown', e => { e.preventDefault(); drag = { which: g.dataset.for, g }; g.classList.add('on'); document.body.classList.add('dragging'); });
    g.addEventListener('dblclick', () => { main.style.removeProperty('--' + g.dataset.for); delete saved[g.dataset.for]; preferences.setItem('gitleaf.panes', JSON.stringify(saved)); if (pdfv.doc) setZoom(pdfv.mode); });
  });
  window.addEventListener('mousemove', e => {
    if (!drag) return; const rect = main.getBoundingClientRect();
    if (drag.which === 'sidew') { const w = Math.max(140, Math.min(rect.width * 0.5, e.clientX - rect.left)); main.style.setProperty('--sidew', w + 'px'); saved.sidew = Math.round(w); }
    else { const w = Math.max(200, Math.min(rect.width * 0.8, rect.right - e.clientX)); main.style.setProperty('--pdfw', w + 'px'); saved.pdfw = Math.round(w); }
  });
  window.addEventListener('mouseup', () => { if (!drag) return; drag.g.classList.remove('on'); document.body.classList.remove('dragging'); drag = null; preferences.setItem('gitleaf.panes', JSON.stringify(saved)); if (pdfv.doc) setZoom(pdfv.mode); });
})();

// ------------------------------------------------------------------ start
Object.assign(window.gitleaf, { cm, view, openProject, openFile, closeTab, save, compile, enterVisual, exitVisual, renderVisual, syncFromVisual, parseDoc, parseBlocks, renderBlocks, serializeBlocks, serializeDoc, setEditorValue, markDirty, markEdited, openNew, showPdf, loadPdf, pdfv, setFocus, toggleSidebar, forwardSync, jumpTo, showProblems, renderProblems, menu, toast });
fetch('/api/health').then(r => r.json()).then(h => { state.health = h; }).catch(() => {});
renderTabs(); updateSave();
loadProjects().catch(e => { $('#startup-error').hidden = false; $('#startup-error').textContent = `GitLeaf could not start: ${e.message}. Reload to retry.`; });
$('#startup-error').hidden = true;
