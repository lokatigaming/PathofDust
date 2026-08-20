// Generates the JavaScript replay-bundle validator from the IDL.
//
//   node tools/gen-bundle-validator.mjs
//
// The IDL (game/schema/replay-bundle.v1.json) is the source of truth. This
// writes game/schema/replay-bundle-validator.js, which is the artifact
// delivered to PathOfDust_Desktop by PR. A Rust test
// (replay_bundle::schema) fails the build if the committed output no longer
// matches what this produces, so the two cannot drift in silence - a shared
// file on its own enforces nothing, because server.mjs and an old desktop
// keep running against a stale copy regardless.
//
// The output embeds the IDL and ships a fixed interpreter for it, rather
// than compiling per-field code. That keeps the generator small enough to
// be obviously correct, and keeps the delivered file readable by whoever
// reviews the PR on the other repo.
//
// House rules on the receiving side: no build step, browser standards and
// Node built-ins only. The output is therefore a plain ES module with no
// imports, usable from a browser, from Node, and from a test runner.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const idlPath = join(here, '..', 'game', 'schema', 'replay-bundle.v1.json');
const outPath = join(here, '..', 'game', 'schema', 'replay-bundle-validator.js');

const idl = JSON.parse(readFileSync(idlPath, 'utf8'));

// Strip $comment keys: they are for whoever edits the IDL, and shipping
// them would put prose about server internals into a public artifact.
const strip = (v) =>
  Array.isArray(v) ? v.map(strip)
    : v && typeof v === 'object'
      ? Object.fromEntries(Object.entries(v).filter(([k]) => k !== '$comment').map(([k, x]) => [k, strip(x)]))
      : v;

const embedded = JSON.stringify(strip(idl), null, 2)
  .split('\n')
  .map((line, i) => (i === 0 ? line : '  ' + line))
  .join('\n');

const out = `// GENERATED FILE - DO NOT EDIT BY HAND.
//
// Source: game/schema/replay-bundle.v1.json in the PathofDust repo.
// Regenerate with: node tools/gen-bundle-validator.mjs
//
// Validates a Path of Dust replay bundle against schema version ${idl.schemaVersion}.
//
// The rules this enforces are deliberately asymmetric. A reader must NEVER
// throw on data it does not recognise: four surfaces version independently
// here, so an old reader and a new writer are always live at the same time.
// Unknown members, unknown event kinds and unknown fields are all ignored
// by design, and a missing optional member is not an error. What IS an
// error is a required member that is absent, a record missing a required
// field, or a field of the wrong type - those mean the writer and reader
// genuinely disagree.
//
// No imports, no build step: browser standards only.

export const SCHEMA = ${embedded};

export const SCHEMA_VERSION = ${idl.schemaVersion};
export const MIN_READER_VERSION = ${idl.minReaderVersion};

const isInt = (v) => typeof v === 'number' && Number.isInteger(v);
const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

function checkField(value, spec, path, errors) {
  if (value === undefined) {
    if (!spec.optional) errors.push(\`\${path}: required field is missing\`);
    return;
  }
  if (value === null) {
    if (!spec.nullable) errors.push(\`\${path}: null is not allowed here\`);
    return;
  }
  switch (spec.type) {
    case 'integer':
      if (!isInt(value)) errors.push(\`\${path}: expected integer, got \${typeof value}\`);
      break;
    case 'number':
      if (!isNum(value)) errors.push(\`\${path}: expected number, got \${typeof value}\`);
      break;
    case 'string':
      if (typeof value !== 'string') errors.push(\`\${path}: expected string, got \${typeof value}\`);
      break;
    case 'boolean':
      if (typeof value !== 'boolean') errors.push(\`\${path}: expected boolean, got \${typeof value}\`);
      break;
    case 'const':
      if (value !== spec.value) errors.push(\`\${path}: expected "\${spec.value}", got "\${value}"\`);
      break;
    case 'enum':
      // An unknown enum value is tolerated on purpose: a newer writer may
      // have added one (curseShare was added to sourceKind exactly this
      // way), and refusing it would break every older reader on contact.
      if (typeof value !== 'string') errors.push(\`\${path}: expected string enum, got \${typeof value}\`);
      break;
    case 'array':
      if (!Array.isArray(value)) { errors.push(\`\${path}: expected array\`); break; }
      if (spec.items === 'string' && !value.every((x) => typeof x === 'string'))
        errors.push(\`\${path}: expected an array of strings\`);
      if (spec.items === 'pair<integer,integer>' && !value.every((x) => Array.isArray(x) && x.length === 2 && isInt(x[0]) && isInt(x[1])))
        errors.push(\`\${path}: expected [integer, integer] pairs\`);
      if (spec.items === 'pair<string,number>' && !value.every((x) => Array.isArray(x) && x.length === 2 && typeof x[0] === 'string' && isNum(x[1])))
        errors.push(\`\${path}: expected [string, number] pairs\`);
      break;
    case 'object':
      if (typeof value !== 'object' || Array.isArray(value)) errors.push(\`\${path}: expected object\`);
      break;
    default:
      // A type this validator predates. Ignore rather than reject.
      break;
  }
}

function checkRecordShape(record, shape, path, errors) {
  for (const name of shape.required || []) {
    if (record[name] === undefined) errors.push(\`\${path}: required field "\${name}" is missing\`);
  }
  for (const [name, spec] of Object.entries(shape.fields || {})) {
    if (record[name] !== undefined || !spec.optional) checkField(record[name], spec, \`\${path}.\${name}\`, errors);
  }
  // Extra fields are NOT an error. See the note at the top of this file.
}

/** Validates one event record against its kind. Unknown kinds are skipped. */
export function validateEvent(event, path, errors) {
  if (!event || typeof event !== 'object') { errors.push(\`\${path}: expected an object\`); return; }
  const shape = SCHEMA.eventKinds[event.kind];
  if (!shape) return; // unknown kind: ignore and continue
  checkRecordShape(event, shape, path, errors);
}

/**
 * Validates a bundle.
 *
 * @param {object} bundle  { manifest, members: { name: data } } - members may
 *                         be partially present; only what you fetched needs
 *                         to be here.
 * @returns {{ ok: boolean, errors: string[] }}
 */
export function validateBundle(bundle) {
  const errors = [];
  if (!bundle || typeof bundle !== 'object') return { ok: false, errors: ['bundle: expected an object'] };

  const manifest = bundle.manifest;
  if (!manifest || typeof manifest !== 'object') {
    return { ok: false, errors: ['manifest: required member is missing'] };
  }
  checkRecordShape(manifest, SCHEMA.manifest, 'manifest', errors);

  if (isInt(manifest.minReaderVersion) && manifest.minReaderVersion > SCHEMA_VERSION) {
    errors.push(
      \`manifest.minReaderVersion \${manifest.minReaderVersion} is newer than this reader (\${SCHEMA_VERSION}) - refuse rather than misread\`,
    );
    return { ok: false, errors };
  }

  const entries = manifest.members && typeof manifest.members === 'object' ? manifest.members : {};
  for (const [name, entry] of Object.entries(entries)) {
    if (!SCHEMA.members[name]) continue; // unknown member: ignore
    checkRecordShape(entry, SCHEMA.manifest.memberEntry, \`manifest.members.\${name}\`, errors);
  }

  for (const [name, def] of Object.entries(SCHEMA.members)) {
    if (!def.requiredMember) continue;
    const entry = entries[name];
    if (!entry) { errors.push(\`manifest.members.\${name}: required member is not listed\`); continue; }
    if (entry.state === 'never-written') errors.push(\`manifest.members.\${name}: required member was never written\`);
  }

  const data = bundle.members || {};
  for (const [name, payload] of Object.entries(data)) {
    const def = SCHEMA.members[name];
    if (!def) continue; // unknown member: ignore
    if (payload === undefined || payload === null) continue;

    if (def.kind === 'eventStream') {
      if (!Array.isArray(payload)) { errors.push(\`members.\${name}: expected an array\`); continue; }
      let previousSeq = -1;
      payload.forEach((event, i) => {
        validateEvent(event, \`members.\${name}[\${i}]\`, errors);
        // The ordering guarantee the whole format rests on: seq must be
        // strictly increasing. It may have GAPS - a thinned copy is an
        // order-preserving subsequence of the archive, and the holes are
        // how a reader knows what thinning removed.
        if (isInt(event?.seq)) {
          if (event.seq <= previousSeq)
            errors.push(\`members.\${name}[\${i}]: seq \${event.seq} does not increase (previous \${previousSeq})\`);
          previousSeq = event.seq;
        }
      });
    } else if (def.kind === 'rollStream') {
      if (!Array.isArray(payload)) { errors.push(\`members.\${name}: expected an array\`); continue; }
      payload.forEach((roll, i) => checkRecordShape(roll, def, \`members.\${name}[\${i}]\`, errors));
    } else if (def.kind === 'array') {
      if (!Array.isArray(payload)) { errors.push(\`members.\${name}: expected an array\`); continue; }
      payload.forEach((item, i) => checkRecordShape(item, def.item, \`members.\${name}[\${i}]\`, errors));
    } else if (def.kind === 'object') {
      if (typeof payload !== 'object' || Array.isArray(payload)) { errors.push(\`members.\${name}: expected an object\`); continue; }
      checkRecordShape(payload, def, \`members.\${name}\`, errors);
    }
  }

  return { ok: errors.length === 0, errors };
}
`;

writeFileSync(outPath, out);
console.log(`wrote ${outPath} (${out.length} bytes) from schema v${idl.schemaVersion}`);
