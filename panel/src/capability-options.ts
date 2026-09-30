// The capability plane's OPTION values (weights, thresholds, caps): the panel's
// mirror of the two rules the DAEMON owns, kept in one module because both of
// them fail SILENTLY when they are got wrong.
//
//  1. VALIDATION. `PUT` validates its body exactly as policy.toml would be
//     validated (crates/daemon/src/capability.rs:577-661 `validate`): every key
//     has one range, the two integer keys are `u32` (so a fractional value is
//     refused before any bound is looked at), and `is_finite` is checked FIRST
//     because NaN is false against every bound. Mirroring that here means an
//     impossible value is refused BEFORE it is sent, next to the field the user
//     typed it into, instead of coming back as a 400 that names one of five
//     numbers.
//
//  2. THE WHOLE-TABLE REPLACE. `CapabilitiesEditor::update`
//     (crates/daemon/src/config.rs:1071-1114) REBUILDS `[capabilities]` from
//     the body it is given: a row the body omits is DELETED from policy.toml,
//     and inside a row `set_or_remove_*` REMOVES every key the entry omits. A
//     body is therefore never "just the field I changed" — it is a complete,
//     explicit statement of what every configured row must still contain. That
//     is why `capabilitiesBody` below is a read-modify-write over the WHOLE
//     response and not a one-key patch.

import type {
  CapabilitiesResponse,
  CapabilityFileEntry,
  CapabilityOptions,
  CapabilityRow,
} from "./api";

/** Every option key a capability may declare, in the registry's order (design
 *  §4.2). A key a capability does not declare is refused by the daemon as an
 *  unknown key (crates/daemon/src/capability.rs:590-604), never ignored. */
export type OptionKey = keyof CapabilityOptions;

export const OPTION_KEYS: OptionKey[] = [
  "weight",
  "min_score",
  "max_per_input",
  "min_confidence",
  "max_docs_per_pass",
];

/** One key's rule, copied from the daemon's `validate` — one entry per bound
 *  that function checks, so the two cannot drift apart unnoticed. */
export interface OptionRule {
  /** true for the `u32` keys: the daemon deserialises these into `u32`, so
   *  `max_per_input = 3.5` never reaches the range check. */
  integer: boolean;
  min: number;
  max: number;
  /** The daemon's OWN phrase from `CapabilityError::BadValue`, shown to the
   *  user as the range (it is the same text a 400 would carry). */
  expectation: string;
}

export const OPTION_RULES: Record<OptionKey, OptionRule> = {
  // capability.rs:608-617
  weight: { integer: false, min: 0, max: 100, expectation: "finite and 0.0..=100.0" },
  // capability.rs:618-627
  min_score: { integer: false, min: 0, max: 1, expectation: "finite and 0.0..=1.0" },
  // capability.rs:628-637
  max_per_input: { integer: true, min: 1, max: 10_000, expectation: "1..=10000" },
  // capability.rs:638-647
  min_confidence: { integer: false, min: 0, max: 1, expectation: "finite and 0.0..=1.0" },
  // capability.rs:648-657
  max_docs_per_pass: { integer: true, min: 1, max: 1000, expectation: "1..=1000" },
};

/** Why a typed value was refused, in the daemon's own two classes: a value it
 *  cannot represent at all (`notFinite` is also the shape test — "0x10" and
 *  "abc" are not numbers either), a value of the wrong number TYPE for a `u32`
 *  key, and a value outside the key's range. */
export type OptionRefusal = "notFinite" | "notInteger" | "outOfRange";

export type OptionCheck =
  | { ok: true; /** `null` = the field is empty = the key stays UNSET. */ value: number | null }
  | { ok: false; refusal: OptionRefusal };

/** A plain decimal literal, with an optional exponent. The SHAPE is checked
 *  before the value because `Number("")` is 0 and `Number("0x10")` is 16: without
 *  this, text that is not a number would silently become one. */
const DECIMAL = /^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/;

/** Mirror of the daemon's per-key check, run BEFORE the write.
 *
 *  An empty field is not an error: it means "unset", and an unset key is
 *  REMOVED from policy.toml rather than written as the registry default (see
 *  `RESET_OPTIONS`). */
export function validateOptionValue(key: OptionKey, raw: string): OptionCheck {
  const text = raw.trim();
  if (text === "") return { ok: true, value: null };
  if (!DECIMAL.test(text)) return { ok: false, refusal: "notFinite" };
  const value = Number(text);
  // `is_finite` FIRST, exactly like the daemon: NaN/Infinity are false against
  // every bound, so a range test alone would wave them through.
  if (!Number.isFinite(value)) return { ok: false, refusal: "notFinite" };
  const rule = OPTION_RULES[key];
  if (rule.integer && !Number.isInteger(value)) return { ok: false, refusal: "notInteger" };
  if (value < rule.min || value > rule.max) return { ok: false, refusal: "outOfRange" };
  return { ok: true, value };
}

/** The option keys `GET /api/v1/capabilities` REPORTS for one row — the keys
 *  this card gives an input to.
 *
 *  The API answers RESOLVED values (`CapabilityPlane::options`): a key whose
 *  value the file does not set is reported as the registry default, or `null`
 *  when the capability's own default is "unset". A capability that declares no
 *  options reports `null` for every key — that is exactly how this module tells
 *  "no options" from "options", and why such a row gets no editor at all
 *  instead of an empty one claiming a knob that does not exist.
 *
 *  The declared-key set is deliberately NOT duplicated here: the registry is the
 *  daemon's, and a second copy in the panel is a thing that drifts. The cost of
 *  that choice is stated rather than hidden: a declared key whose default is
 *  unset (there is one today, `min_score` on the semantic recall leg) shows an
 *  input only once a value for it is in policy.toml. */
export function reportedOptions(row: CapabilityRow): OptionKey[] {
  return OPTION_KEYS.filter((key) => row.options?.[key] != null);
}

/** A row's option values as the `[capabilities.<id>]` keys to write, with
 *  `overrides` applied. A key not reported and not overridden is omitted, and
 *  an omitted key is REMOVED from that row by the daemon's editor.
 *
 *  The base values are the ones the API reports, i.e. the ones the user is
 *  looking at. Re-emitting them is what keeps a key that the file DOES carry
 *  (even one set to its registry default, which the API cannot distinguish from
 *  "unset") from being dropped by a write about some other key. */
export function optionValues(
  row: CapabilityRow,
  overrides: Partial<Record<OptionKey, number | null>> = {},
): CapabilityFileEntry {
  const out: CapabilityFileEntry = {};
  for (const key of OPTION_KEYS) {
    const override = overrides[key];
    const value = override === undefined ? row.options?.[key] : override;
    if (value != null) out[key] = value;
  }
  return out;
}

/** The option set that leaves every one of a row's keys UNSET: `{}` on the
 *  wire, which `set_or_remove_*` turns into REMOVING each key from policy.toml —
 *  not writing the registry default in as an explicit opinion. */
export const RESET_OPTIONS: CapabilityFileEntry = {};

/** One row's entry in the next `PUT` body. */
export interface RowEdit {
  /** The row's flag AFTER this change (never "the flag I am changing"). */
  enabled: boolean;
  /** The row's COMPLETE option set after this change: the daemon removes every
   *  key this omits, so it must never be "just the field I touched". */
  options: CapabilityFileEntry;
}

/** The `PUT /api/v1/capabilities` body for ONE change of ONE row.
 *
 *  READ-MODIFY-WRITE, because the daemon's `CapabilitiesEditor::update`
 *  REPLACES the whole `[capabilities]` table: a row absent from this body is
 *  DELETED from policy.toml, and inside a row `set_or_remove_*`
 *  (crates/daemon/src/config.rs:1100-1105) removes every key the body omits. A
 *  body carrying only the edited row would therefore reset every other
 *  configured row's `weight`/`min_score` and drop its `enabled` key — one
 *  weight edit, a silently rewritten config file.
 *
 *  So every row the file already configures (`configured === "file"`) is
 *  re-emitted with its resolved options, and only the edited row changes value.
 *  A row nobody has written (`configured !== "file"`) is left OUT on purpose:
 *  it resolves to its registry default, which is the value `GET` reported and
 *  the value the editor shows. The edited row is always present — writing it is
 *  the point. */
export function capabilitiesBody(
  data: CapabilitiesResponse,
  id: string,
  edit: RowEdit,
): Record<string, CapabilityFileEntry> {
  const out: Record<string, CapabilityFileEntry> = {};
  for (const row of data.capabilities) {
    const editing = row.id === id;
    if (!editing && row.configured !== "file") continue;
    out[row.id] = editing
      ? { enabled: edit.enabled, ...edit.options }
      : { enabled: row.enabled, ...optionValues(row) };
  }
  return out;
}
