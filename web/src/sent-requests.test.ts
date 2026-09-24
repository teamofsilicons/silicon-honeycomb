import assert from 'node:assert/strict';
import test from 'node:test';
import { loadSentRequests } from './sent-requests';
import type { request } from './api';

test('sent requests include all managed pages and completed history, independent of incoming review eligibility', async () => {
  const calls: string[] = [];
  const read = (async (path: string) => {
    calls.push(path);
    if (path.endsWith('page=1')) return { total: 2, items: [{ app_id: 'first', name: 'First' }] };
    if (path.endsWith('page=2')) return { total: 2, items: [{ app_id: 'second', name: 'Second' }] };
    if (path.includes('first')) return { items: [{ id: 'one', revision: 2, state: 'awaiting_validator', gates: [] }, { id: 'old', revision: 1, state: 'denied', gates: [] }] };
    if (path.includes('second')) return { items: [{ id: 'two', revision: 1, state: 'published', gates: [] }] };
    throw Error('Unexpected request: '+path);
  }) as typeof request;
  const items = await loadSentRequests(read);
  assert.deepEqual(items.map(item => item.state), ['awaiting_validator', 'denied', 'published']);
  assert.equal(items[2].app.app_id, 'second');
  assert.equal(calls.length, 4);
});

test('a failed app history cannot masquerade as a complete inbox', async () => {
  const read = (async (path: string) => {
    if (path.includes('managed=true')) return { total: 2, items: [{ app_id: 'first' }, { app_id: 'second' }] };
    if (path.includes('first')) return { items: [] };
    throw Error('Access changed');
  }) as typeof request;
  await assert.rejects(loadSentRequests(read), /Access changed/);
});

test('nonadvancing pagination stops with a recoverable error', async () => {
  const read = (async () => ({total:2, items:[{app_id:'first'}]})) as typeof request;
  await assert.rejects(loadSentRequests(read), /Refresh sent requests/);
});
