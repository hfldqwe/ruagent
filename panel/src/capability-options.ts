// The capability plane's OPTION values (weights, thresholds, caps) as the panel
// edits them — driven by what the DAEMON DECLARES, never by a table copied into
// this file.
//
// Two daemon-owned rules govern every write here, and both are mirrored from the
// RESPONSE (`GET /api/v1/capabilities`) rather than re-spelled:
//
//  1. VALIDATION, from `options_schema`. Each entry publishes one DECLARED key,
//     its JSON `kind` ("float" | "uint"), its accepted range (`min`/`max`) and
//     the daemon's own `expectation` phrase — all generated from the single
//     per-key table `CapabilityPlane::validate` enforces
//     (crates/daemon/src/capability.rs). So the panel cannot show a range the
//     daemon does not accept, and a refusal prints the daemon's phrase instead of
//     a second wording that could drift from it. This module used to carry its own
//     copy of that table — a key list plus per-key bounds and phrases — and it is
//     DELETED: nothing replaces it, and every bound used below comes off the
//     response.
//
//  2. THE WHOLE-TABLE REPLACE. `PUT /api/v1/capabilities` replaces the whole
//     `[capabilities]` table, and `CapabilitiesEditor::update`
//     (crates/daemon/src/config.rs:1071-1114) REMOVES every row and every option
//     key an entry omits. A body is therefore never "just the field I changed": it
//     must restate every configured row — and, with `options_set`, exactly the
//     keys the FILE carries, so that editing one row cannot persist another row's
//     REGISTRY DEFAULTS as the user's own choice.

import type {
  CapabilitiesResponse,
  CapabilityFileEntry,
  CapabilityRow,
  OptionSchemaEntry,
} from "./api";

/** Why a typed value was refused.
 *
 *  The classes are the panel's (it decides what to say); the RANGE in every
 *  message is the daemon's `expectation` string, verbatim.
 *
 *  `zeroWeight` is the ONE cross-field member: it is not about the value of a
 *  single key, so it cannot come from the per-key schema — see
 *  `zeroWeightEnabled`. */
export type OptionRefusal = "notFinite" | "notInteger" | "outOfRange" | "zeroWeight";

export type OptionCheck =
  | { ok: true; /** `null` = the field is empty = the key stays UNSET. */ value: number | null }
  | { ok: false; refusal: OptionRefusal };

/** A plain decimal literal, with an optional exponent. The SHAPE is checked
 *  before the value because `Number("")` is 0 and `Number("0x10")` is 16: text
 *  that is not a number must not silently become one. */
const DECIMAL = /^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/;

/** The daemon's per-key check, evaluated from the daemon's OWN schema entry.
 *
 *  An empty field is not an error: it means "unset", and an unset key is REMOVED
 *  from policy.toml (`editedOptions` omits it) rather than written as the
 *  registry default. */
export function validateOptionValue(entry: OptionSchemaEntry, raw: string): OptionCheck {
  const text = raw.trim();
  if (text === "") return { ok: true, value: null };
  if (!DECIMAL.test(text)) return { ok: false, refusal: "notFinite" };
  const value = Number(text);
  // `is_finite` FIRST, exactly like the daemon's `validate`: NaN and Infinity are
  // false against every bound, so a range test alone would wave them through.
  if (!Number.isFinite(value)) return { ok: false, refusal: "notFinite" };
  // `kind` is what says whether an INTEGER is required — the bounds cannot, since
  // a uint's bounds are integral but so are some float keys' (min_confidence 0).
  if (entry.kind === "uint" && !Number.isInteger(value)) {
    return { ok: false, refusal: "notInteger" };
  }
  if (value < entry.min || value > entry.max) return { ok: false, refusal: "outOfRange" };
  return { ok: true, value };
}

/** A row's RESOLVED value for one key, read by the daemon's own key name. The
 *  key set is the registry's, so it is read as a map here (and nowhere else)
 *  instead of being spelled out as a TS interface that could go stale. */
function resolved(row: CapabilityRow, key: string): number | null {
  return (row.options as unknown as Record<string, number | null>)[key] ?? null;
}

/** The value one input shows — and the meaning of leaving it empty.
 *
 *  * the FILE carries the key (`options_set`) → the resolved value, which IS the
 *    file's own value;
 *  * the file does not carry it → the registry's declared `default`, or `null`
 *    (an EMPTY field) where the registry declares none — today that is exactly
 *    one key, `min_score` on the semantic recall leg, which this schema is what
 *    makes reachable at all.
 *
 *  An empty field means UNSET, never "write the default in": a key the user has
 *  not touched emits nothing, so policy.toml keeps saying "unset" and the daemon
 *  keeps applying the default. */
export function optionValue(row: CapabilityRow, entry: OptionSchemaEntry): number | null {
  if (row.options_set?.[entry.key] === true) return resolved(row, entry.key);
  return entry.default;
}

/** Whether the FILE carries one declared key. */
export function fileCarries(row: CapabilityRow, entry: OptionSchemaEntry): boolean {
  return row.options_set?.[entry.key] === true;
}

/** The OPTION keys (never `enabled`) the file carries for one row, with the
 *  values it holds. This is the read half of read-modify-write: what the FILE
 *  says, not what the config resolves to. */
export function fileOptions(row: CapabilityRow): CapabilityFileEntry {
  const out: CapabilityFileEntry = {};
  for (const entry of row.options_schema ?? []) {
    if (!fileCarries(row, entry)) continue;
    const value = resolved(row, entry.key);
    if (value != null) out[entry.key] = value;
  }
  return out;
}

/** The option set one row's entry carries after setting `values` on top of what
 *  the FILE already carries. A value of `null` CLEARS the key (the daemon removes
 *  what the entry omits); a key absent from `values` keeps the file's state, and
 *  a key the file does not carry stays absent — the registry default is never
 *  materialized by an edit that did not name it. */
export function editedOptions(
  row: CapabilityRow,
  values: Record<string, number | null>,
): CapabilityFileEntry {
  const out = fileOptions(row);
  for (const [key, value] of Object.entries(values)) {
    if (value == null) delete out[key];
    else out[key] = value;
  }
  return out;
}

/** The option set that leaves every one of a row's keys UNSET: `{}` on the wire,
 *  which `set_or_remove_*` turns into REMOVING each key from policy.toml — not
 *  writing the registry default in as an explicit opinion. */
export const RESET_OPTIONS: CapabilityFileEntry = {};

/** The `enabled` value an OPTION edit must re-state: the row's current flag when
 *  the file carries it, `undefined` (= leave the key alone) when it does not.
 *
 *  Writing `enabled` for a row whose file never named it would turn a default
 *  into a persisted opinion — the same defect `options_set` exists to prevent,
 *  one key over. A SWITCH FLIP is different and states it explicitly. */
export function preservedEnabled(row: CapabilityRow): boolean | undefined {
  return row.options_set?.enabled === true ? row.enabled : undefined;
}

/** One row's entry in the next `PUT` body. */
export interface RowEdit {
  /** The flag to write, or `undefined` to leave the file's `enabled` key exactly
   *  as it is. */
  enabled?: boolean;
  /** The row's COMPLETE option set after the edit: the daemon removes every key
   *  this omits. */
  options: CapabilityFileEntry;
}

/** The panel's fallback for a weight nothing sets.
 *
 *  The daemon's write door (`zero_weight_leg`, crates/daemon/src/capability.rs) does
 *  NOT forward a bare `Option`: it reads the plane's RESOLVED weight —
 *  `CapabilityPlane::options`, documented as "the resolved options (defaults merged
 *  with the file)" and spelled `weight: f.weight.or(defaults.weight)` — so a NUMBER
 *  always reaches the runtime, and the numbers in play are the REGISTRY's declared
 *  defaults (`CapabilitySpec::defaults`: 1.0 on both memory legs and on
 *  `recall_leg_knowledge_fts`, 2.0 on `recall_leg_knowledge_semantic`). The runtime's
 *  OWN per-leg default (`MemoryLegs::resolve`'s `map_or(1.0, ..)`, `LegConfig::resolve`'s
 *  `FUSION` weight) therefore never fires on this path: it takes a `None` the plane
 *  cannot produce for a row that declares `weight`.
 *
 *  This constant is a last resort for the same reason — such a row ALWAYS resolves to
 *  a number, and its schema `default` is never null (the registry's ONE null-default
 *  key is `min_score` on `recall_leg_memory_semantic`), which is what the two `??`
 *  chains below fall back through. */
const DEFAULT_WEIGHT = 1.0;

function zeroed(enabled: boolean, weight: number): boolean {
  return enabled && weight <= 0;
}

/** Would this edit CREATE the state the daemon refuses — an ENABLED row at weight
 *  0 — where it did not exist before?
 *
 *  THE ONE CLIENT-SIDE RULE. The daemon's schema is per key (`kind`/`min`/`max`),
 *  so a rule spanning `enabled` and `weight` cannot be mirrored from data, and the
 *  API publishes no cross-field constraint to read. The daemon enforces it at its
 *  write door (`zero_weight_leg`, crates/daemon/src/capability.rs) for EVERY leg the
 *  recall runtime weighs — four today: memory semantic, memory fts, knowledge
 *  semantic, knowledge keyword — and it enumerates none of them: the door asks the
 *  runtime (`RecallLegConfig::resolve` -> `check_weights`,
 *  crates/knowledge/src/rrf.rs, which refuses an enabled leg at weight <= 0) which
 *  legs carry a weight at all. The door answers with that path's own sentence, so a
 *  client that skipped this guard still cannot persist the state.
 *
 *  Two details are mirrored deliberately, because they are what make the two doors
 *  agree rather than merely resemble each other:
 *   * the TRANSITION shape (`after && !before`, the `unconfirmed_llm_enable`
 *     shape), so a policy.toml that already carries `enabled + weight 0.0` — which
 *     still boots, and whose reads still 400 — stays EDITABLE from the panel
 *     instead of being bricked by its own history;
 *   * the weights themselves: the resolved value for the current state, and for
 *     the new state either the value being written or the declared default when
 *     the edit REMOVES the key (a removal falls back to the default, exactly as
 *     the plane resolves it).
 *
 *  The panel applies the rule to every row that declares `weight`, and the daemon's
 *  door refuses the same set — not because two lists agree, but because the door asks
 *  the runtime which legs it weighs. Measured: the two surfaces' ACCEPTED SETS over
 *  all 11 capability ids are identical, so what differs is not WHETHER a state is
 *  refused but WHERE and in WHAT WORDS. The panel pre-checks from the payload of its
 *  last `GET` (an input turns red and no request is sent for that row), while the door
 *  evaluates the LIVE plane at `PUT` time — `before` is what the daemon holds, not
 *  what the panel last saw. A concurrent write, or an operator editing policy.toml,
 *  can therefore make the two disagree for the duration of that race, and the DOOR IS
 *  AUTHORITATIVE: a state it refuses is refused whatever the panel believed.
 *  The daemon's sentence is not reproduced either — it is built in Rust from the
 *  leg's display label, which the API does not expose as data, and a copy of that
 *  sentence is the kind of duplicated fact this module deletes; a client that skips
 *  this pre-check simply meets the daemon's own sentence in the `PUT`'s 400. */
export function zeroWeightEnabled(row: CapabilityRow, edit: RowEdit): boolean {
  const weightEntry = (row.options_schema ?? []).find((e) => e.key === "weight");
  if (!weightEntry) return false;
  const before = zeroed(
    row.enabled,
    resolved(row, "weight") ?? weightEntry.default ?? DEFAULT_WEIGHT,
  );
  const written = edit.options["weight"];
  const afterWeight = typeof written === "number" ? written : (weightEntry.default ?? DEFAULT_WEIGHT);
  return zeroed(edit.enabled ?? row.enabled, afterWeight) && !before;
}

/** The `PUT /api/v1/capabilities` body for ONE change of ONE row.
 *
 *  READ-MODIFY-WRITE, because the daemon's `CapabilitiesEditor::update`
 *  REPLACES the whole `[capabilities]` table: a row absent from this body is
 *  DELETED from policy.toml, and inside a row `set_or_remove_*`
 *  (crates/daemon/src/config.rs:1100-1105) removes every key the body omits. A
 *  body carrying only the edited row would therefore delete every other
 *  configured row outright — one weight edit, a silently rewritten config file.
 *
 *  So every row the file already configures (`configured === "file"`) is
 *  re-emitted with `fileOptions` — the keys the FILE carries, not the values the
 *  config resolves to — and only the edited row changes. A row nobody has written
 *  (`configured !== "file"`) is left OUT on purpose: it resolves to its registry
 *  default, which is the value `GET` reported and the value the editor shows. The
 *  edited row is always present — writing it is the point. */
export function capabilitiesBody(
  data: CapabilitiesResponse,
  id: string,
  edit: RowEdit,
): Record<string, CapabilityFileEntry> {
  const out: Record<string, CapabilityFileEntry> = {};
  for (const row of data.capabilities) {
    const editing = row.id === id;
    if (!editing && row.configured !== "file") continue;
    if (!editing) {
      const entry = fileOptions(row);
      if (row.options_set?.enabled === true) entry.enabled = row.enabled;
      out[row.id] = entry;
      continue;
    }
    out[row.id] = { ...edit.options };
    if (edit.enabled !== undefined) out[row.id].enabled = edit.enabled;
  }
  return out;
}
