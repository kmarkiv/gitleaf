import { test } from 'node:test';
import assert from 'node:assert/strict';
import { SaveQueue } from '../frontend/save-queue.mjs';

function setup() {
  const document = { content: 'first', revision: 1, dirty: true };
  const requests = [], failures = [];
  let release;
  const queue = new SaveQueue({
    snapshot: () => document.dirty ? [{ ...document }] : [],
    write: async snapshot => { requests.push(snapshot); if (requests.length === 1) await new Promise(resolve => { release = resolve; }); },
    commit: snapshot => { if (snapshot.revision === document.revision) document.dirty = false; },
    failed: (_, error) => failures.push(error),
  });
  return { document, requests, failures, queue, release: () => release() };
}

test('typing while saving persists the newer revision before reporting success', async () => {
  const fixture = setup();
  const pending = fixture.queue.save();
  fixture.document.content = 'second'; fixture.document.revision++;
  fixture.release();
  assert.equal(await pending, true);
  assert.deepEqual(fixture.requests.map(r => r.content), ['first', 'second']);
  assert.equal(fixture.document.dirty, false);
});

test('overlapping autosave and explicit save share one ordered operation', async () => {
  const fixture = setup();
  const first = fixture.queue.save(), second = fixture.queue.save();
  assert.equal(first, second);
  fixture.release(); await first;
  assert.equal(fixture.requests.length, 1);
});

test('failed saves retain dirty edits and can be retried', async () => {
  const fixture = setup();
  fixture.queue.write = async () => { throw new Error('disk full'); };
  assert.equal(await fixture.queue.save(), false);
  assert.equal(fixture.document.dirty, true);
  assert.equal(fixture.failures[0].message, 'disk full');
  fixture.queue.write = async () => {};
  assert.equal(await fixture.queue.save(), true);
  assert.equal(fixture.document.dirty, false);
});

test('a continuously changing document never authorizes a compile of stale source', async () => {
  const fixture = setup();
  fixture.queue.write = async () => { fixture.document.revision++; };
  assert.equal(await fixture.queue.save(), false);
  assert.equal(fixture.document.dirty, true);
});
