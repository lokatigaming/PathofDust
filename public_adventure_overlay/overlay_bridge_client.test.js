// Bridge-fix (2026-08-20) client-side test - the ONE JavaScript test in
// this otherwise pure-Rust test suite, because this specific logic
// (window.postMessage envelope handling) is client-only and never
// touches the server, so no Rust integration test can exercise it.
// Run directly: `node public_adventure_overlay/overlay_bridge_client.test.js`
// (no runner/framework - a plain assert-and-exit-nonzero script, since
// nothing else in this codebase has ever needed one).
//
// Extracts and executes the REAL `feed=parent` block straight out of
// overlay.html (not a hand-copied re-implementation) so this can't
// silently drift out of sync with the shipped code - the exact bug
// class this whole release exists to fix (the bridge going dormant
// with the page still loading/rendering fine, so nothing else would
// have caught it).
const assert = require('assert');
const fs = require('fs');
const path = require('path');

const html = fs.readFileSync(path.join(__dirname, 'overlay.html'), 'utf8');
const startMarker = 'const feedMode = new URLSearchParams';
const start = html.indexOf(startMarker);
assert(start !== -1, 'feedMode block not found in overlay.html - this test needs updating to match a real structural change, not silently skip');
const end = html.indexOf('</script>', start);
const block = html.slice(start, end);

function runBlock(searchString) {
  const calls = { handled: [], posted: [] };
  const listeners = [];
  const sandbox = {
    URLSearchParams,
    location: { search: searchString },
    window: {
      addEventListener: (type, fn) => {
        if (type === 'message') listeners.push(fn);
      },
      parent: { postMessage: (msg, origin) => calls.posted.push({ msg, origin }) },
    },
    handleOverlayMessage: (payload) => calls.handled.push(payload),
    connect: (useCompression) => calls.handled.push({ connectCalled: true, useCompression }),
  };
  const fn = new Function(...Object.keys(sandbox), block);
  fn(...Object.values(sandbox));
  return { calls, listeners };
}

let failures = 0;
function test(name, fn) {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (err) {
    failures++;
    console.error(`FAIL - ${name}`);
    console.error(`  ${err.message}`);
  }
}

test('an enveloped frame from pod-app is unwrapped to its own data', () => {
  const { listeners, calls } = runBlock('?feed=parent');
  const payload = { type: 'state', stage: 1 };
  listeners[0]({ data: { source: 'pod-app', v: 1, type: 'ws', data: payload } });
  assert.deepStrictEqual(calls.handled, [payload]);
});

test('a bare state/encounter payload (no envelope) is still handled unchanged', () => {
  const { listeners, calls } = runBlock('?feed=parent');
  const payload = { type: 'encounter', foo: 1 };
  listeners[0]({ data: payload });
  assert.deepStrictEqual(calls.handled, [payload]);
});

test('an envelope-shaped message from any other source is ignored', () => {
  const { listeners, calls } = runBlock('?feed=parent');
  listeners[0]({ data: { source: 'someone-else', type: 'ws', data: { type: 'state' } } });
  assert.strictEqual(calls.handled.length, 0);
});

test('the 2.6.0 ready handshake is posted to the parent exactly once', () => {
  const { calls } = runBlock('?feed=parent');
  assert.strictEqual(calls.posted.length, 1);
  assert.strictEqual(calls.posted[0].msg.source, 'pod-overlay');
  assert.strictEqual(calls.posted[0].msg.type, 'ready');
});

test('default (no feed=parent) path is unaffected - no listener, no ready post, connect(true) still called', () => {
  const { listeners, calls } = runBlock('');
  assert.strictEqual(listeners.length, 0);
  assert.strictEqual(calls.posted.length, 0);
  assert(calls.handled.some((c) => c.connectCalled === true && c.useCompression === true));
});

if (failures > 0) {
  console.error(`\n${failures} test(s) failed`);
  process.exit(1);
} else {
  console.log('\nall tests passed');
}
