// GENERATED FILE - DO NOT EDIT BY HAND.
//
// Source: game/schema/replay-bundle.v1.json in the PathofDust repo.
// Regenerate with: node tools/gen-bundle-validator.mjs
//
// Validates a Path of Dust replay bundle against schema version 1.
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

export const SCHEMA = {
    "schemaVersion": 1,
    "minReaderVersion": 1,
    "readerRules": {
      "unknownMembersAreIgnored": true,
      "unknownEventKindsAreIgnored": true,
      "unknownFieldsAreIgnored": true,
      "missingOptionalMemberIsNotAnError": true,
      "readersMustNotThrow": true
    },
    "memberStates": {
      "values": [
        "present",
        "aggregated",
        "expired",
        "never-written"
      ]
    },
    "tiers": {
      "values": [
        "public",
        "participant",
        "operator"
      ]
    },
    "manifest": {
      "required": [
        "schemaVersion",
        "minReaderVersion",
        "fightId",
        "startedAtUnixMs",
        "realDurationMs",
        "displayDurationMs",
        "pinned",
        "members"
      ],
      "fields": {
        "schemaVersion": {
          "type": "integer"
        },
        "minReaderVersion": {
          "type": "integer"
        },
        "fightId": {
          "type": "string"
        },
        "startedAtUnixMs": {
          "type": "integer"
        },
        "realDurationMs": {
          "type": "integer"
        },
        "displayDurationMs": {
          "type": "integer"
        },
        "pinned": {
          "type": "boolean"
        },
        "members": {
          "type": "object"
        }
      },
      "memberEntry": {
        "required": [
          "v",
          "state",
          "tier"
        ],
        "fields": {
          "v": {
            "type": "integer"
          },
          "state": {
            "type": "enum",
            "values": [
              "present",
              "aggregated",
              "expired",
              "never-written"
            ]
          },
          "tier": {
            "type": "enum",
            "values": [
              "public",
              "participant",
              "operator"
            ]
          },
          "bytes": {
            "type": "integer",
            "optional": true
          },
          "sha256": {
            "type": "string",
            "optional": true
          },
          "expiredAtUnixMs": {
            "type": "integer",
            "optional": true
          },
          "pinnedShape": {
            "type": "boolean",
            "optional": true
          }
        }
      }
    },
    "members": {
      "core": {
        "v": 1,
        "requiredMember": true,
        "tier": "public",
        "kind": "object",
        "fields": {
          "kind": {
            "type": "string"
          },
          "stage": {
            "type": "integer"
          },
          "won": {
            "type": "boolean"
          },
          "participants": {
            "type": "array",
            "items": "string"
          },
          "units": {
            "type": "array",
            "items": "object"
          },
          "displayDurationMs": {
            "type": "integer"
          },
          "realDurationMs": {
            "type": "integer"
          },
          "loot": {
            "type": "array",
            "items": "object"
          },
          "broken": {
            "type": "array",
            "items": "object"
          },
          "enemyName": {
            "type": "string",
            "nullable": true
          },
          "enemyCount": {
            "type": "integer",
            "nullable": true
          },
          "retreated": {
            "type": "array",
            "items": "string"
          },
          "bossSprites": {
            "type": "array",
            "items": "string"
          },
          "summary": {
            "type": "object"
          },
          "bossStats": {
            "type": "array",
            "items": "object"
          },
          "startedAtUnixMs": {
            "type": "integer"
          }
        }
      },
      "replay": {
        "v": 1,
        "requiredMember": true,
        "tier": "public",
        "kind": "eventStream",
        "sequenceKey": "seq",
        "eventKinds": [
          "attack",
          "heal",
          "defeat",
          "skillCast"
        ],
        "sourceKindExcludes": [
          "dot"
        ]
      },
      "buffs": {
        "v": 1,
        "requiredMember": false,
        "tier": "participant",
        "kind": "eventStream",
        "sequenceKey": "seq",
        "eventKinds": [
          "shield",
          "buffSnapshot"
        ]
      },
      "dot": {
        "v": 1,
        "requiredMember": false,
        "tier": "participant",
        "kind": "eventStream",
        "sequenceKey": "seq",
        "eventKinds": [
          "attack"
        ],
        "sourceKindFilter": "dot",
        "aggregatable": true
      },
      "rolls": {
        "v": 1,
        "requiredMember": false,
        "tier": "operator",
        "kind": "rollStream",
        "fields": {
          "eventId": {
            "type": "integer"
          },
          "hitId": {
            "type": "integer"
          },
          "causedBy": {
            "type": "integer",
            "optional": true,
            "nullable": true
          },
          "atMs": {
            "type": "integer"
          },
          "category": {
            "type": "string"
          },
          "source": {
            "type": "string"
          },
          "actor": {
            "type": "string"
          },
          "target": {
            "type": "string"
          },
          "probability": {
            "type": "number",
            "optional": true
          },
          "succeeded": {
            "type": "boolean",
            "optional": true
          },
          "magnitude": {
            "type": "number",
            "optional": true
          }
        }
      },
      "playerVitals": {
        "v": 1,
        "requiredMember": true,
        "tier": "public",
        "pinnedShape": true,
        "kind": "array",
        "item": {
          "required": [
            "id",
            "hpSamples"
          ],
          "fields": {
            "id": {
              "type": "string"
            },
            "hpSamples": {
              "type": "array",
              "items": "pair<integer,integer>"
            },
            "diedAtMs": {
              "type": "integer",
              "optional": true,
              "nullable": true
            }
          }
        }
      }
    },
    "eventKinds": {
      "attack": {
        "required": [
          "seq",
          "kind",
          "atMs",
          "attacker",
          "target",
          "damage",
          "targetHpAfter"
        ],
        "fields": {
          "seq": {
            "type": "integer"
          },
          "kind": {
            "type": "const",
            "value": "attack"
          },
          "atMs": {
            "type": "integer"
          },
          "attacker": {
            "type": "string"
          },
          "target": {
            "type": "string"
          },
          "damage": {
            "type": "integer"
          },
          "unmitigatedDamage": {
            "type": "integer",
            "optional": true
          },
          "targetHpAfter": {
            "type": "integer"
          },
          "isCrit": {
            "type": "boolean",
            "optional": true
          },
          "evaded": {
            "type": "boolean",
            "optional": true
          },
          "hitId": {
            "type": "integer",
            "optional": true
          },
          "sourceKind": {
            "type": "enum",
            "values": [
              "direct",
              "splash",
              "dot",
              "reflect",
              "curseShare",
              "environmental"
            ],
            "optional": true,
            "default": "direct"
          }
        }
      },
      "heal": {
        "required": [
          "seq",
          "kind",
          "atMs",
          "healer",
          "target",
          "amount",
          "targetHpAfter"
        ],
        "fields": {
          "seq": {
            "type": "integer"
          },
          "kind": {
            "type": "const",
            "value": "heal"
          },
          "atMs": {
            "type": "integer"
          },
          "healer": {
            "type": "string"
          },
          "target": {
            "type": "string"
          },
          "amount": {
            "type": "integer"
          },
          "targetHpAfter": {
            "type": "integer"
          },
          "isRevive": {
            "type": "boolean",
            "optional": true,
            "default": false
          }
        }
      },
      "defeat": {
        "required": [
          "seq",
          "kind",
          "atMs",
          "unit"
        ],
        "fields": {
          "seq": {
            "type": "integer"
          },
          "kind": {
            "type": "const",
            "value": "defeat"
          },
          "atMs": {
            "type": "integer"
          },
          "unit": {
            "type": "string"
          }
        }
      },
      "skillCast": {
        "required": [
          "seq",
          "kind",
          "atMs",
          "unit",
          "skill"
        ],
        "fields": {
          "seq": {
            "type": "integer"
          },
          "kind": {
            "type": "const",
            "value": "skillCast"
          },
          "atMs": {
            "type": "integer"
          },
          "unit": {
            "type": "string"
          },
          "skill": {
            "type": "string"
          }
        }
      },
      "shield": {
        "required": [
          "seq",
          "kind",
          "atMs",
          "healer",
          "target",
          "amount"
        ],
        "fields": {
          "seq": {
            "type": "integer"
          },
          "kind": {
            "type": "const",
            "value": "shield"
          },
          "atMs": {
            "type": "integer"
          },
          "healer": {
            "type": "string"
          },
          "target": {
            "type": "string"
          },
          "amount": {
            "type": "integer"
          }
        }
      },
      "buffSnapshot": {
        "required": [
          "seq",
          "kind",
          "atMs",
          "unit",
          "buffs"
        ],
        "fields": {
          "seq": {
            "type": "integer"
          },
          "kind": {
            "type": "const",
            "value": "buffSnapshot"
          },
          "atMs": {
            "type": "integer"
          },
          "unit": {
            "type": "string"
          },
          "buffs": {
            "type": "array",
            "items": "pair<string,number>"
          }
        }
      }
    }
  };

export const SCHEMA_VERSION = 1;
export const MIN_READER_VERSION = 1;

const isInt = (v) => typeof v === 'number' && Number.isInteger(v);
const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

function checkField(value, spec, path, errors) {
  if (value === undefined) {
    if (!spec.optional) errors.push(`${path}: required field is missing`);
    return;
  }
  if (value === null) {
    if (!spec.nullable) errors.push(`${path}: null is not allowed here`);
    return;
  }
  switch (spec.type) {
    case 'integer':
      if (!isInt(value)) errors.push(`${path}: expected integer, got ${typeof value}`);
      break;
    case 'number':
      if (!isNum(value)) errors.push(`${path}: expected number, got ${typeof value}`);
      break;
    case 'string':
      if (typeof value !== 'string') errors.push(`${path}: expected string, got ${typeof value}`);
      break;
    case 'boolean':
      if (typeof value !== 'boolean') errors.push(`${path}: expected boolean, got ${typeof value}`);
      break;
    case 'const':
      if (value !== spec.value) errors.push(`${path}: expected "${spec.value}", got "${value}"`);
      break;
    case 'enum':
      // An unknown enum value is tolerated on purpose: a newer writer may
      // have added one (curseShare was added to sourceKind exactly this
      // way), and refusing it would break every older reader on contact.
      if (typeof value !== 'string') errors.push(`${path}: expected string enum, got ${typeof value}`);
      break;
    case 'array':
      if (!Array.isArray(value)) { errors.push(`${path}: expected array`); break; }
      if (spec.items === 'string' && !value.every((x) => typeof x === 'string'))
        errors.push(`${path}: expected an array of strings`);
      if (spec.items === 'pair<integer,integer>' && !value.every((x) => Array.isArray(x) && x.length === 2 && isInt(x[0]) && isInt(x[1])))
        errors.push(`${path}: expected [integer, integer] pairs`);
      if (spec.items === 'pair<string,number>' && !value.every((x) => Array.isArray(x) && x.length === 2 && typeof x[0] === 'string' && isNum(x[1])))
        errors.push(`${path}: expected [string, number] pairs`);
      break;
    case 'object':
      if (typeof value !== 'object' || Array.isArray(value)) errors.push(`${path}: expected object`);
      break;
    default:
      // A type this validator predates. Ignore rather than reject.
      break;
  }
}

function checkRecordShape(record, shape, path, errors) {
  for (const name of shape.required || []) {
    if (record[name] === undefined) errors.push(`${path}: required field "${name}" is missing`);
  }
  for (const [name, spec] of Object.entries(shape.fields || {})) {
    if (record[name] !== undefined || !spec.optional) checkField(record[name], spec, `${path}.${name}`, errors);
  }
  // Extra fields are NOT an error. See the note at the top of this file.
}

/** Validates one event record against its kind. Unknown kinds are skipped. */
export function validateEvent(event, path, errors) {
  if (!event || typeof event !== 'object') { errors.push(`${path}: expected an object`); return; }
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
      `manifest.minReaderVersion ${manifest.minReaderVersion} is newer than this reader (${SCHEMA_VERSION}) - refuse rather than misread`,
    );
    return { ok: false, errors };
  }

  const entries = manifest.members && typeof manifest.members === 'object' ? manifest.members : {};
  for (const [name, entry] of Object.entries(entries)) {
    if (!SCHEMA.members[name]) continue; // unknown member: ignore
    checkRecordShape(entry, SCHEMA.manifest.memberEntry, `manifest.members.${name}`, errors);
  }

  for (const [name, def] of Object.entries(SCHEMA.members)) {
    if (!def.requiredMember) continue;
    const entry = entries[name];
    if (!entry) { errors.push(`manifest.members.${name}: required member is not listed`); continue; }
    if (entry.state === 'never-written') errors.push(`manifest.members.${name}: required member was never written`);
  }

  const data = bundle.members || {};
  for (const [name, payload] of Object.entries(data)) {
    const def = SCHEMA.members[name];
    if (!def) continue; // unknown member: ignore
    if (payload === undefined || payload === null) continue;

    if (def.kind === 'eventStream') {
      if (!Array.isArray(payload)) { errors.push(`members.${name}: expected an array`); continue; }
      let previousSeq = -1;
      payload.forEach((event, i) => {
        validateEvent(event, `members.${name}[${i}]`, errors);
        // The ordering guarantee the whole format rests on: seq must be
        // strictly increasing. It may have GAPS - a thinned copy is an
        // order-preserving subsequence of the archive, and the holes are
        // how a reader knows what thinning removed.
        if (isInt(event?.seq)) {
          if (event.seq <= previousSeq)
            errors.push(`members.${name}[${i}]: seq ${event.seq} does not increase (previous ${previousSeq})`);
          previousSeq = event.seq;
        }
      });
    } else if (def.kind === 'rollStream') {
      if (!Array.isArray(payload)) { errors.push(`members.${name}: expected an array`); continue; }
      payload.forEach((roll, i) => checkRecordShape(roll, def, `members.${name}[${i}]`, errors));
    } else if (def.kind === 'array') {
      if (!Array.isArray(payload)) { errors.push(`members.${name}: expected an array`); continue; }
      payload.forEach((item, i) => checkRecordShape(item, def.item, `members.${name}[${i}]`, errors));
    } else if (def.kind === 'object') {
      if (typeof payload !== 'object' || Array.isArray(payload)) { errors.push(`members.${name}: expected an object`); continue; }
      checkRecordShape(payload, def, `members.${name}`, errors);
    }
  }

  return { ok: errors.length === 0, errors };
}
