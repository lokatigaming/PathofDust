// Tests for the relay Worker's /api/tip routes and the capture-ID dedupe
// shared with the webhook path.
//
//   node cloudflare-paypal-relay/worker.test.mjs
//
// PayPal is mocked by replacing globalThis.fetch; KV is an in-memory Map.
// No dependencies: node:test, node:assert only.
import { test } from 'node:test';
import assert from 'node:assert/strict';

import worker, { validateTipAmount, TWITCH_CLIENT_ID, BROADCASTER_LOGIN, REMOVALS_PER_MINUTE } from './worker.js';

function memoryKv() {
  const store = new Map();
  return {
    store,
    async get(key) { return store.has(key) ? store.get(key) : null; },
    async put(key, value) { store.set(key, value); },
    async delete(key) { store.delete(key); },
    async list({ prefix }) {
      return { keys: [...store.keys()].filter((k) => k.startsWith(prefix)).map((name) => ({ name })) };
    },
  };
}

function makeEnv() {
  return {
    TIPS_KV: memoryKv(),
    PAYPAL_CLIENT_ID: 'test-id',
    PAYPAL_CLIENT_SECRET: 'test-secret',
    PAYPAL_WEBHOOK_ID: 'test-webhook',
    RELAY_TOKEN: 'test-relay-token',
    PAYPAL_API_BASE: 'https://paypal.test',
  };
}

// Routes each mocked PayPal call by path; records every call made.
function mockPaypal({ order, capture, captureStatus = 200 } = {}) {
  const calls = [];
  globalThis.fetch = async (url, init = {}) => {
    const path = new URL(url).pathname;
    calls.push({ path, body: init.body });
    const json = (body, status = 200) => new Response(JSON.stringify(body), { status });
    if (path === '/v1/oauth2/token') return json({ access_token: 'tok' });
    if (path === '/v1/notifications/verify-webhook-signature') return json({ verification_status: 'SUCCESS' });
    if (path === '/v2/checkout/orders') return json(order || { id: 'ORDER1', status: 'CREATED' }, 201);
    if (/^\/v2\/checkout\/orders\/[^/]+\/capture$/.test(path)) return json(capture, captureStatus);
    throw new Error(`unexpected PayPal call ${path}`);
  };
  return calls;
}

const post = (path, body) =>
  new Request(`https://lokati.net${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: typeof body === 'string' ? body : JSON.stringify(body),
  });

const completedCapture = (captureId, value = '5.00', status = 'COMPLETED') => ({
  id: 'ORDER1',
  status,
  payer: { name: { given_name: 'Pay', surname: 'Pal' } },
  purchase_units: [{ payments: { captures: [{ id: captureId, status, amount: { currency_code: 'USD', value } }] } }],
});

const tipKeys = (env) => [...env.TIPS_KV.store.keys()].filter((k) => k.startsWith('tip:'));

async function drain(env) {
  const resp = await worker.fetch(
    new Request('https://relay.test/pending-tips', { headers: { Authorization: `Bearer ${env.RELAY_TOKEN}` } }),
    env,
  );
  return resp.json();
}

// ----------------------------------------------------------- amount range

test('validateTipAmount accepts in-range amounts and normalises them', () => {
  assert.equal(validateTipAmount(1), '1.00');
  assert.equal(validateTipAmount('5'), '5.00');
  assert.equal(validateTipAmount('12.5'), '12.50');
  assert.equal(validateTipAmount(' 500.00 '), '500.00');
});

test('validateTipAmount rejects out-of-range and malformed input', () => {
  for (const bad of [0, 0.99, '0.5', 500.01, '501', -5, '-5', '1e3', '5.001', 'abc', '', null, undefined, {}, [], NaN, Infinity, '5,00', '0x10']) {
    assert.equal(validateTipAmount(bad), null, `should reject ${JSON.stringify(bad)}`);
  }
});

test('POST /api/tip/order rejects a bad amount without calling PayPal', async () => {
  const calls = mockPaypal();
  const env = makeEnv();
  for (const body of [{ amount: 0 }, { amount: '9999' }, { amount: 'ten' }, {}, 'not json']) {
    const resp = await worker.fetch(post('/api/tip/order', body), env);
    assert.equal(resp.status, 400, `body ${JSON.stringify(body)}`);
  }
  assert.equal(calls.length, 0);
  assert.equal(env.TIPS_KV.store.size, 0);
});

test('POST /api/tip/order creates the order for the validated amount and parks name/message', async () => {
  const calls = mockPaypal();
  const env = makeEnv();
  const resp = await worker.fetch(post('/api/tip/order', { amount: '7.5', name: '  Viewer  ', message: 'gl hf' }), env);
  assert.equal(resp.status, 200);
  assert.deepEqual(await resp.json(), { id: 'ORDER1' });
  assert.equal(resp.headers.get('Access-Control-Allow-Origin'), 'https://lokati.net');

  const create = calls.find((c) => c.path === '/v2/checkout/orders');
  const sent = JSON.parse(create.body);
  assert.equal(sent.intent, 'CAPTURE');
  assert.deepEqual(sent.purchase_units[0].amount, { currency_code: 'USD', value: '7.50' });
  assert.deepEqual(JSON.parse(env.TIPS_KV.store.get('order:ORDER1')), { name: 'Viewer', message: 'gl hf' });
});

// ---------------------------------------------------------------- capture

test('capture writes a tip only on COMPLETED, with amount/currency from PayPal', async () => {
  mockPaypal({ capture: completedCapture('CAP1', '5.00') });
  const env = makeEnv();
  env.TIPS_KV.store.set('order:ORDER1', JSON.stringify({ name: 'Viewer', message: 'hi' }));

  // The page can't influence the amount: anything it sends besides orderID is ignored.
  const resp = await worker.fetch(post('/api/tip/capture', { orderID: 'ORDER1', amount: 9999, currency: 'EUR' }), env);
  assert.equal(resp.status, 200);
  assert.deepEqual(tipKeys(env), ['tip:CAP1']);
  assert.deepEqual(JSON.parse(env.TIPS_KV.store.get('tip:CAP1')), {
    id: 'CAP1', name: 'Viewer', amount: 5, currency: 'USD', message: 'hi',
  });
  assert.ok(env.TIPS_KV.store.has('seen:CAP1'));
  assert.ok(!env.TIPS_KV.store.has('order:ORDER1'));
});

test('capture writes nothing when PayPal does not report COMPLETED', async () => {
  for (const capture of [completedCapture('CAP2', '5.00', 'PENDING'), completedCapture('CAP3', '5.00', 'DECLINED'), { id: 'ORDER1', status: 'COMPLETED', purchase_units: [] }]) {
    mockPaypal({ capture });
    const env = makeEnv();
    const resp = await worker.fetch(post('/api/tip/capture', { orderID: 'ORDER1' }), env);
    assert.equal(resp.status, 202);
    assert.deepEqual(tipKeys(env), []);
  }

  mockPaypal({ capture: { name: 'UNPROCESSABLE_ENTITY' }, captureStatus: 422 });
  const env = makeEnv();
  const resp = await worker.fetch(post('/api/tip/capture', { orderID: 'ORDER1' }), env);
  assert.equal(resp.status, 502);
  assert.deepEqual(tipKeys(env), []);
});

test('capture rejects a malformed orderID without calling PayPal', async () => {
  const calls = mockPaypal();
  const env = makeEnv();
  for (const body of [{}, { orderID: '' }, { orderID: '../../v1/x' }, { orderID: 42 }]) {
    const resp = await worker.fetch(post('/api/tip/capture', body), env);
    assert.equal(resp.status, 400);
  }
  assert.equal(calls.length, 0);
});

// ------------------------------------------------------------------ dedupe

const webhookFor = (captureId) =>
  post('/paypal-webhook', {
    event_type: 'PAYMENT.CAPTURE.COMPLETED',
    resource: { id: captureId, amount: { value: '5.00', currency_code: 'USD' } },
  });

test('same capture via direct path then webhook records exactly one tip', async () => {
  mockPaypal({ capture: completedCapture('CAPX') });
  const env = makeEnv();
  await worker.fetch(post('/api/tip/capture', { orderID: 'ORDER1' }), env);
  const hook = await worker.fetch(webhookFor('CAPX'), env);
  assert.equal(hook.status, 200);
  assert.equal(await hook.text(), 'Duplicate');
  const tips = await drain(env);
  assert.equal(tips.length, 1);
  assert.equal(tips[0].id, 'CAPX');
});

test('webhook arriving after the bot already drained the direct tip is still skipped', async () => {
  mockPaypal({ capture: completedCapture('CAPY') });
  const env = makeEnv();
  await worker.fetch(post('/api/tip/capture', { orderID: 'ORDER1' }), env);
  assert.equal((await drain(env)).length, 1);
  await worker.fetch(webhookFor('CAPY'), env);
  assert.equal((await drain(env)).length, 0);
});

test('webhook first then direct path lands on the same key — still one tip', async () => {
  mockPaypal({ capture: completedCapture('CAPZ') });
  const env = makeEnv();
  env.TIPS_KV.store.set('order:ORDER1', JSON.stringify({ name: 'Viewer', message: 'hi' }));
  await worker.fetch(webhookFor('CAPZ'), env);
  await worker.fetch(post('/api/tip/capture', { orderID: 'ORDER1' }), env);
  const tips = await drain(env);
  assert.equal(tips.length, 1);
  assert.equal(tips[0].name, 'Viewer');
});

test('a webhook for a capture not seen before still queues (backup path intact)', async () => {
  mockPaypal();
  const env = makeEnv();
  await worker.fetch(webhookFor('CAPW'), env);
  assert.deepEqual(tipKeys(env), ['tip:CAPW']);
  assert.ok(env.TIPS_KV.store.has('seen:CAPW'));
});

// ----------------------------------------------- /api/playlist (item 43)

// Twitch's validate endpoint, mocked: token -> what Twitch says about it.
function mockTwitch(tokens) {
  const calls = [];
  globalThis.fetch = async (url, init = {}) => {
    const u = new URL(url);
    if (u.host !== 'id.twitch.tv' || u.pathname !== '/oauth2/validate') throw new Error(`unexpected call ${url}`);
    const token = (init.headers.Authorization || '').replace(/^OAuth /, '');
    calls.push(token);
    const info = tokens[token];
    if (!info) return new Response(JSON.stringify({ status: 401, message: 'invalid access token' }), { status: 401 });
    return new Response(JSON.stringify({ scopes: [], expires_in: 3600, ...info }), { status: 200 });
  };
  return calls;
}

const TOKENS = {
  'viewer-tok': { client_id: TWITCH_CLIENT_ID, login: 'Viewer', user_id: '1' },
  'other-app-tok': { client_id: 'someone-elses-app', login: 'viewer', user_id: '1' },
  'app-tok': { client_id: TWITCH_CLIENT_ID },
  'bc-tok': { client_id: TWITCH_CLIENT_ID, login: BROADCASTER_LOGIN, user_id: '2' },
};

function playlistEnv() {
  const env = makeEnv();
  env.TIPS_KV.store.set('feed:playlists', JSON.stringify({
    viewer: { displayName: 'Viewer', songs: [{ videoId: 'vidA' }, { videoId: 'vidA' }, { videoId: 'vidB' }] },
    someone: { displayName: 'Someone', songs: [{ videoId: 'vidS' }] },
  }));
  return env;
}

const playlistReq = (path, { token, body, method = body ? 'POST' : 'GET' } = {}) => {
  const headers = { 'Content-Type': 'application/json' };
  if (token) headers.Authorization = `Bearer ${token}`;
  return new Request(`https://lokati.net${path}`, { method, headers, body: body ? JSON.stringify(body) : undefined });
};

const queued = (env) =>
  [...env.TIPS_KV.store.entries()].filter(([k]) => k.startsWith('plrm:')).map(([, v]) => JSON.parse(v));

test('/me: no token, invalid token, and another app\'s token are all 401', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  for (const token of [undefined, 'expired-or-invalid', 'other-app-tok', 'app-tok']) {
    const resp = await worker.fetch(playlistReq('/api/playlist/me', { token }), env);
    assert.equal(resp.status, 401, `token ${token}`);
  }
});

test('/me: a valid token answers the login Twitch gives, lowercased', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  const viewer = await (await worker.fetch(playlistReq('/api/playlist/me', { token: 'viewer-tok' }), env)).json();
  assert.deepEqual(viewer, { login: 'viewer', isBroadcaster: false });
  const bc = await (await worker.fetch(playlistReq('/api/playlist/me', { token: 'bc-tok' }), env)).json();
  assert.deepEqual(bc, { login: BROADCASTER_LOGIN, isBroadcaster: true });
});

test('/remove: no token, invalid, or another client_id is 401 and queues nothing', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  for (const token of [undefined, 'expired-or-invalid', 'other-app-tok']) {
    const resp = await worker.fetch(playlistReq('/api/playlist/remove', { token, body: { videoId: 'vidA' } }), env);
    assert.equal(resp.status, 401, `token ${token}`);
  }
  assert.equal(queued(env).length, 0);
});

test('/remove: own playlist queues exactly one removal', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  const resp = await worker.fetch(playlistReq('/api/playlist/remove', { token: 'viewer-tok', body: { videoId: 'vidA' } }), env);
  assert.equal(resp.status, 200);
  const q = queued(env);
  assert.equal(q.length, 1);
  assert.equal(q[0].login, 'viewer');
  assert.equal(q[0].videoId, 'vidA');
  assert.ok(q[0].id && typeof q[0].at === 'number');
  assert.equal((await resp.json()).id, q[0].id);
});

test('/remove: a non-broadcaster sending someone else\'s owner still edits only their own', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  // vidS is only on someone's list, so honouring owner would have queued it.
  const denied = await worker.fetch(playlistReq('/api/playlist/remove', { token: 'viewer-tok', body: { videoId: 'vidS', owner: 'someone' } }), env);
  assert.equal(denied.status, 404);
  const own = await worker.fetch(playlistReq('/api/playlist/remove', { token: 'viewer-tok', body: { videoId: 'vidB', owner: 'someone' } }), env);
  assert.equal(own.status, 200);
  assert.deepEqual(queued(env).map((r) => r.login), ['viewer']);
});

test('/remove: the broadcaster with owner edits that playlist', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  const resp = await worker.fetch(playlistReq('/api/playlist/remove', { token: 'bc-tok', body: { videoId: 'vidS', owner: 'Someone' } }), env);
  assert.equal(resp.status, 200);
  assert.deepEqual(queued(env).map((r) => [r.login, r.videoId]), [['someone', 'vidS']]);
});

test('/remove: unknown videoId is 404 and nothing is queued', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  const resp = await worker.fetch(playlistReq('/api/playlist/remove', { token: 'viewer-tok', body: { videoId: 'nope' } }), env);
  assert.equal(resp.status, 404);
  assert.equal(queued(env).length, 0);
});

test('/remove: the 31st removal in a minute is 429', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  for (let i = 0; i < REMOVALS_PER_MINUTE; i++) {
    const resp = await worker.fetch(playlistReq('/api/playlist/remove', { token: 'viewer-tok', body: { videoId: 'vidA' } }), env);
    assert.equal(resp.status, 200, `removal ${i + 1}`);
  }
  const resp = await worker.fetch(playlistReq('/api/playlist/remove', { token: 'viewer-tok', body: { videoId: 'vidA' } }), env);
  assert.equal(resp.status, 429);
  assert.equal(queued(env).length, REMOVALS_PER_MINUTE);
});

test('/pending-playlist-removals refuses without RELAY_TOKEN and drains with it', async () => {
  mockTwitch(TOKENS);
  const env = playlistEnv();
  await worker.fetch(playlistReq('/api/playlist/remove', { token: 'viewer-tok', body: { videoId: 'vidA' } }), env);
  for (const auth of [undefined, 'Bearer viewer-tok', 'Bearer wrong']) {
    const headers = auth ? { Authorization: auth } : {};
    const resp = await worker.fetch(new Request('https://relay.test/pending-playlist-removals', { headers }), env);
    assert.equal(resp.status, 401);
  }
  assert.equal(queued(env).length, 1);
  const resp = await worker.fetch(
    new Request('https://relay.test/pending-playlist-removals', { headers: { Authorization: `Bearer ${env.RELAY_TOKEN}` } }),
    env,
  );
  const removals = await resp.json();
  assert.equal(removals.length, 1);
  assert.equal(removals[0].videoId, 'vidA');
  assert.equal(queued(env).length, 0);
});
