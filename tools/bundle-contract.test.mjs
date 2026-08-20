// Cross-repo golden contract test for the replay bundle.
//
//   node tools/bundle-contract.test.mjs
//
// This exact file is the one PROPOSED into PathOfDust_Desktop's CI, so that
// the server's fixtures and the desktop's reader are checked against each
// other on both sides of the boundary. A shared schema file enforces
// nothing on its own: server.mjs and an old desktop keep running against a
// stale copy of it regardless. Only a test that fails a build does.
//
// It covers three separate things, and the middle one is the one people
// forget:
//
//   1. The golden bundle validates. (Writer and schema agree.)
//   2. FORWARD COMPATIBILITY: unknown members, unknown event kinds, unknown
//      fields and unknown enum values must all be accepted. Four surfaces
//      version independently here, so a newer writer meets an older reader
//      as a matter of routine, not as an edge case.
//   3. Genuine disagreement is still rejected: a missing required member, a
//      missing required field, a wrong type, a non-increasing seq.
//
// No dependencies: node:test, node:assert, node:fs only.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdtempSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';

import { validateBundle, SCHEMA, SCHEMA_VERSION } from '../game/schema/replay-bundle-validator.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const fixturePath = join(here, '..', 'game', 'tests', 'fixtures', 'replay_bundle', 'golden-bundle.v1.json');

const golden = () => JSON.parse(readFileSync(fixturePath, 'utf8'));

// ---------------------------------------------------------------- 1. valid

test('the golden bundle validates clean', () => {
  const { ok, errors } = validateBundle(golden());
  assert.deepEqual(errors, [], 'golden bundle must produce no errors');
  assert.equal(ok, true);
});

test('every event kind in the schema appears somewhere in the golden bundle', () => {
  const b = golden();
  const seen = new Set();
  for (const name of ['replay', 'buffs', 'dot']) for (const e of b.members[name] || []) seen.add(e.kind);
  // dot carries attacks, so the set is the kinds, not the members.
  for (const kind of Object.keys(SCHEMA.eventKinds)) {
    assert.ok(seen.has(kind), `golden bundle should exercise "${kind}" - it does not`);
  }
});

test('the pinned playerVitals shape is exactly {id, hpSamples, diedAtMs?}', () => {
  // Pinned byte-for-byte: the external consumer is the desktop app, and this
  // member's version never advances without an explicit migration agreed
  // with it. A field added here silently is the failure being guarded.
  const item = SCHEMA.members.playerVitals.item;
  assert.deepEqual(Object.keys(item.fields).sort(), ['diedAtMs', 'hpSamples', 'id']);
  assert.deepEqual(item.required.sort(), ['hpSamples', 'id']);
  assert.equal(SCHEMA.members.playerVitals.pinnedShape, true);

  for (const v of golden().members.playerVitals) {
    for (const key of Object.keys(v)) {
      assert.ok(key in item.fields, `playerVitals carries unpinned field "${key}"`);
    }
    assert.ok(v.hpSamples.every((s) => Array.isArray(s) && s.length === 2), 'hpSamples must be [atMs, hp] pairs');
  }
});

// ------------------------------------------------- 2. forward compatibility

test('an unknown member is ignored, not rejected', () => {
  const b = golden();
  b.manifest.members.somethingNew = { v: 1, state: 'present', tier: 'public' };
  b.members.somethingNew = [{ whatever: true }];
  assert.deepEqual(validateBundle(b).errors, []);
});

test('an unknown event kind is ignored, not rejected', () => {
  const b = golden();
  b.members.replay.push({ seq: 99, kind: 'teleport', atMs: 1000, unit: 'lokati_gaming' });
  assert.deepEqual(validateBundle(b).errors, []);
});

test('an unknown field on a known event is ignored, not rejected', () => {
  const b = golden();
  b.members.replay[1].brandNewField = 'from a newer writer';
  assert.deepEqual(validateBundle(b).errors, []);
});

test('an unknown enum value is accepted', () => {
  // curseShare was added to sourceKind after the first readers shipped;
  // rejecting an unrecognised value would have broken all of them.
  const b = golden();
  b.members.replay[1].sourceKind = 'somethingAddedLater';
  assert.deepEqual(validateBundle(b).errors, []);
});

test('a missing OPTIONAL member is not an error', () => {
  const b = golden();
  delete b.members.dot;
  delete b.members.buffs;
  assert.deepEqual(validateBundle(b).errors, []);
});

test('an expired member is a state, not an error', () => {
  const b = golden();
  assert.equal(b.manifest.members.rolls.state, 'expired');
  assert.deepEqual(validateBundle(b).errors, [], 'expired must be distinguishable from absent, and legal');
});

// ------------------------------------------------- 3. genuine disagreement

test('a required member that was never written is an error', () => {
  const b = golden();
  b.manifest.members.replay.state = 'never-written';
  const { ok, errors } = validateBundle(b);
  assert.equal(ok, false);
  assert.ok(errors.some((e) => e.includes('never written')), errors.join('\n'));
});

test('a required member missing from the manifest is an error', () => {
  const b = golden();
  delete b.manifest.members.playerVitals;
  const { ok, errors } = validateBundle(b);
  assert.equal(ok, false);
  assert.ok(errors.some((e) => e.includes('playerVitals')), errors.join('\n'));
});

test('a missing required field on an event is an error', () => {
  const b = golden();
  delete b.members.replay[1].targetHpAfter;
  const { ok, errors } = validateBundle(b);
  assert.equal(ok, false);
  assert.ok(errors.some((e) => e.includes('targetHpAfter')), errors.join('\n'));
});

test('a wrong field type is an error', () => {
  const b = golden();
  b.members.replay[1].damage = '84404';
  const { ok, errors } = validateBundle(b);
  assert.equal(ok, false);
  assert.ok(errors.some((e) => e.includes('damage')), errors.join('\n'));
});

test('a non-increasing seq is an error, but gaps are not', () => {
  // Gaps are load-bearing: a thinned copy is an order-preserving
  // subsequence of the archive, and the holes are how a reader knows what
  // thinning removed. Going backwards, though, means the stream cannot be
  // reassembled at all.
  const withGaps = golden();
  assert.deepEqual(validateBundle(withGaps).errors, [], 'the golden bundle already has seq gaps and must pass');

  const backwards = golden();
  backwards.members.replay[2].seq = 0;
  const { ok, errors } = validateBundle(backwards);
  assert.equal(ok, false);
  assert.ok(errors.some((e) => e.includes('seq')), errors.join('\n'));
});

test('a bundle demanding a newer reader is refused rather than misread', () => {
  const b = golden();
  b.manifest.minReaderVersion = SCHEMA_VERSION + 1;
  const { ok, errors } = validateBundle(b);
  assert.equal(ok, false);
  assert.ok(errors.some((e) => e.includes('minReaderVersion')), errors.join('\n'));
});

test('a malformed bundle never throws', () => {
  // The rule readers must obey: degrade and report, never throw.
  for (const junk of [null, undefined, 42, 'string', [], {}, { manifest: null }, { manifest: {} }]) {
    assert.doesNotThrow(() => validateBundle(junk), `threw on ${JSON.stringify(junk)}`);
  }
});

// ------------------------------------------------------------- 4. no drift

test('the committed validator is in sync with the IDL', () => {
  // The generator is the only thing allowed to write the validator. If the
  // IDL moved and nobody regenerated, the two repos would be validating
  // against different contracts while both looked fine.
  const committed = readFileSync(join(here, '..', 'game', 'schema', 'replay-bundle-validator.mjs'), 'utf8');
  const scratch = mkdtempSync(join(tmpdir(), 'pod-bundle-'));
  const backup = join(scratch, 'validator.js');
  writeFileSync(backup, committed);

  execFileSync(process.execPath, [join(here, 'gen-bundle-validator.mjs')], { stdio: 'pipe' });
  const regenerated = readFileSync(join(here, '..', 'game', 'schema', 'replay-bundle-validator.mjs'), 'utf8');

  assert.equal(
    regenerated,
    committed,
    'replay-bundle-validator.mjs is stale - run: node tools/gen-bundle-validator.mjs',
  );
});

// ------------------------------------------ 5. the Rust writer's real output

test('the bundle the Rust writer actually produced validates', () => {
  // writer-output.v1.json is committed verbatim from build_bundle, and a Rust
  // test asserts the writer still reproduces it byte for byte. So this is the
  // real cross-repo contract: one artifact, checked from the writer's side
  // there and the reader's side here, rather than two prose descriptions that
  // agree until they quietly don't.
  const produced = JSON.parse(
    readFileSync(join(here, '..', 'game', 'tests', 'fixtures', 'replay_bundle', 'writer-output.v1.json'), 'utf8'),
  );
  const { ok, errors } = validateBundle(produced);
  assert.deepEqual(errors, [], 'the writer produced a bundle this reader rejects');
  assert.equal(ok, true);
});

test("the writer's pinned playerVitals keeps its key order, not just its keys", () => {
  // Byte-level, deliberately. Members are serialized once and carried as raw
  // bytes so key order is stable; routing them through a serde_json::Value
  // sorts keys alphabetically and rewrites {id, hpSamples, diedAtMs} as
  // {diedAtMs, hpSamples, id} - valid JSON, identical values, and a broken
  // pin. That regression was real and this is what catches it.
  const raw = readFileSync(join(here, '..', 'game', 'tests', 'fixtures', 'replay_bundle', 'writer-output.v1.json'), 'utf8');
  const vitals = JSON.parse(raw).members.playerVitals;
  assert.ok(Array.isArray(vitals) && vitals.length > 0, 'the golden bundle must carry playerVitals');
  assert.deepEqual(Object.keys(vitals[0]), ['id', 'hpSamples', 'diedAtMs'], 'pinned key ORDER changed');
});
