// The decisions pane's slate logic: reading a cadence:ask-questions slate (slate.json), the
// person's picks on each card, and the response written back (response.json). No `herdr`
// import, so it runs under node for the tests in tests/fork/decisions.
//
// FORK(decisions): this is a port of cadence's plugins/ask/hooks/slate.ts (the Claude Code
// decisions pane). The slate and response shapes are cadence:ask-questions' own
// (references/slate-authoring.md, references/response-handling.md), and the rules mirror its
// scripts/serve-decisions.py `validate_spec`. Keep the three in step.

/**
 * Control and format characters (bidi marks, zero-widths, tags), default-ignorable,
 * private-use, unassigned and surrogate code points, and blanks that draw as nothing: drawn as a
 * space, never passed through. A space, a newline, U+2028 and U+2029 are left to `clean`.
 */
const HIDDEN = /(?![\n \u2028\u2029])[\p{Cc}\p{Cf}\p{Co}\p{Cs}\p{Cn}\p{Zl}\p{Zp}\p{Zs}\p{Default_Ignorable_Code_Point}\u2800\ufffc\ufff9-\ufffb]/gu;

/** One line or block of slate text, cleaned and cut. Newlines survive only where `keepLines`. */
export function clean(s, max, keepLines = false) {
  let t = String(s ?? "").replace(HIDDEN, " ");
  t = t.replace(/[\u2028\u2029]/g, keepLines ? "\n" : " ");
  if (!keepLines) t = t.replace(/\n/g, " ");
  t = t.trim();
  const points = [...t];
  return points.length > max ? `${points.slice(0, max - 1).join("")}…` : t;
}

/** A block shown verbatim in monospace: hidden characters become spaces, a tab four spaces;
 *  leading spaces and blank lines stay, so a diagram keeps its shape. */
export function cleanBlock(s, max) {
  const t = String(s ?? "").replace(/\r\n?/g, "\n").replace(/\t/g, "    ").replace(HIDDEN, " ").replace(/[\u2028\u2029]/g, "\n").replace(/\s+$/, "");
  const points = [...t];
  return points.length > max ? `${points.slice(0, max - 1).join("")}…` : t;
}

export const SLATE_MAX = {
  decisions: 20,
  options: 8,
  compare: 3,
  frames: 6,
  title: 200,
  intro: 1200,
  context: 2000,
  label: 160,
  point: 240,
  points: 20,
  issues: 20,
  issue: 40,
  frameLabel: 120,
  frame: 4000,
  candidate: 6000,
  placeholder: 160,
  dir: 300,
  notes: 4000
};
export const OTHER_ID = "__other__";
export const OTHER_LABEL = "Something else (see notes)";
const RESERVED_DECISION_IDS = new Set(["__slate_notes__"]);
const ID = /^[A-Za-z0-9][A-Za-z0-9_-]*$/;
const cut = (s) => {
  const points = [...s];
  return points.length > SLATE_MAX.notes ? points.slice(0, SLATE_MAX.notes).join("") : s;
};
const isObj = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
const isText = (v) => typeof v === "string" && v.trim() !== "";

class SlateError extends Error {
}
const fail = (msg) => {
  throw new SlateError(msg);
};
function strings(v, where, field, most, each) {
  if (v === undefined)
    return [];
  if (!Array.isArray(v) || !v.every((x) => typeof x === "string"))
    return fail(`${where} '${field}' must be a list of strings`);
  if (v.length > most)
    return fail(`${where} '${field}' has ${v.length} entries; keep it to ${most}`);
  return v.map((x) => clean(x, each)).filter((x) => x !== "");
}
function frames(d, where) {
  if (!("walkthrough" in d))
    return [];
  const raw = d["walkthrough"];
  if (!Array.isArray(raw) || raw.length === 0)
    return fail(`${where} 'walkthrough' must be a non-empty list of frames`);
  if (raw.length > SLATE_MAX.frames)
    return fail(`${where} has ${raw.length} walkthrough frames; keep it to ${SLATE_MAX.frames}`);
  return raw.map((f, k) => {
    const spot = `${where}.walkthrough[${k}]`;
    if (!isObj(f))
      return fail(`${spot} must be an object with 'label' and 'content'`);
    for (const field of ["label", "content"])
      if (!isText(f[field]))
        fail(`${spot} needs a non-empty '${field}' string`);
    const unknown = Object.keys(f).filter((k2) => k2 !== "label" && k2 !== "content");
    if (unknown.length > 0)
      fail(`${spot} has unknown field(s) ${JSON.stringify(unknown.sort())}; a frame takes 'label' and 'content'`);
    return { label: clean(f["label"], SLATE_MAX.frameLabel), content: cleanBlock(f["content"], SLATE_MAX.frame) };
  });
}
function card(d, i, seen) {
  const where = `decisions[${i}]`;
  if (!isObj(d))
    return fail(`${where} must be an object`);
  for (const field of ["id", "title", "context"])
    if (!isText(d[field]))
      fail(`${where} needs a non-empty '${field}' string`);
  const id = String(d["id"]);
  if (seen.has(id))
    fail(`${where} repeats id ${JSON.stringify(id)}; ids must be unique`);
  if (RESERVED_DECISION_IDS.has(id))
    fail(`${where} uses reserved id ${JSON.stringify(id)}`);
  if (!ID.test(id) || id.length > 64)
    fail(`${where} id ${JSON.stringify(id)} must be kebab-case (letters, digits, - and _), at most 64 characters`);
  seen.add(id);
  const issues = strings(d["issues"], where, "issues", SLATE_MAX.issues, SLATE_MAX.issue);
  const walkthrough = frames(d, where);
  const rawType = d["type"] ?? "options";
  if (rawType !== "options" && rawType !== "compare" && rawType !== "text")
    return fail(`${where} 'type' must be one of ["compare","options","text"], got ${JSON.stringify(rawType)}`);
  const type = rawType;
  const select = d["select"] ?? "single";
  if (select !== "single" && select !== "multi")
    fail(`${where} 'select' must be one of ["multi","single"], got ${JSON.stringify(select)}`);
  if (type !== "options" && "select" in d)
    fail(`${where} type ${JSON.stringify(type)} takes no 'select'; only an options card does`);
  let placeholder = "";
  if ("placeholder" in d) {
    if (type !== "text")
      fail(`${where} 'placeholder' only applies to a text card`);
    if (typeof d["placeholder"] !== "string")
      fail(`${where} 'placeholder' must be a string`);
    placeholder = clean(d["placeholder"], SLATE_MAX.placeholder);
  }
  const base = { id, title: clean(d["title"], SLATE_MAX.title), context: clean(d["context"], SLATE_MAX.context, true), issues, walkthrough, type, multi: select === "multi", placeholder };
  if (type === "text") {
    if ("options" in d)
      fail(`${where} type "text" takes no 'options'`);
    return { ...base, options: [] };
  }
  const raw = d["options"];
  if (!Array.isArray(raw) || raw.length === 0)
    return fail(`${where} needs a non-empty 'options' array`);
  if (type === "compare" && (raw.length < 2 || raw.length > SLATE_MAX.compare))
    fail(`${where} type 'compare' needs 2 or 3 candidates, got ${raw.length}`);
  if (type === "options" && raw.length > SLATE_MAX.options)
    fail(`${where} has ${raw.length} options; keep it to ${SLATE_MAX.options}`);
  const ids = new Set;
  const options = raw.map((o, j) => {
    const spot = `${where}.options[${j}]`;
    if (!isObj(o))
      return fail(`${spot} must be an object`);
    for (const field of ["id", "label"])
      if (!isText(o[field]))
        fail(`${spot} needs a non-empty '${field}' string`);
    const oid = String(o["id"]);
    if (oid === OTHER_ID)
      fail(`${spot} uses reserved option id "${OTHER_ID}"; every card gets that one automatically`);
    if (ids.has(oid))
      fail(`${spot} repeats option id ${JSON.stringify(oid)}`);
    if (!ID.test(oid) || oid.length > 64)
      fail(`${spot} id ${JSON.stringify(oid)} must be kebab-case (letters, digits, - and _), at most 64 characters`);
    ids.add(oid);
    const pros = strings(o["pros"], spot, "pros", SLATE_MAX.points, SLATE_MAX.point);
    const cons = strings(o["cons"], spot, "cons", SLATE_MAX.points, SLATE_MAX.point);
    let content = "";
    if (type === "compare") {
      if (!isText(o["content"]))
        fail(`${spot} needs a non-empty 'content' string on a compare card`);
      content = cleanBlock(o["content"], SLATE_MAX.candidate);
    } else if ("content" in o)
      fail(`${spot} 'content' only applies to a compare card`);
    return { id: oid, label: clean(o["label"], SLATE_MAX.label), recommended: o["recommended"] === true, pros, cons, content };
  });
  return { ...base, options };
}
export function parseSlate(spec) {
  try {
    if (!isObj(spec))
      return fail("slate must be a JSON object");
    const raw = spec["decisions"];
    if (!Array.isArray(raw) || raw.length === 0)
      return fail("slate needs a non-empty 'decisions' array");
    if (raw.length > SLATE_MAX.decisions)
      return fail(`slate has ${raw.length} decisions; the pane takes ${SLATE_MAX.decisions}. Split it, or use the browser dashboard`);
    for (const field of ["title", "intro"])
      if (field in spec && typeof spec[field] !== "string")
        fail(`slate '${field}' must be a string`);
    const mode = spec["mode"] ?? "wait";
    if (mode !== "wait" && mode !== "continue")
      fail(`slate 'mode' must be one of ["continue","wait"], got ${JSON.stringify(mode)}`);
    const seen = new Set;
    const decisions = raw.map((d, i) => card(d, i, seen));
    const title = clean(spec["title"], SLATE_MAX.title) || "Decisions";
    return { ok: true, spec: { title, intro: clean(spec["intro"], SLATE_MAX.intro, true), mode, decisions } };
  } catch (err) {
    if (err instanceof SlateError)
      return { ok: false, error: err.message };
    throw err;
  }
}
const SLATE_DIR = /^\/(?:[\p{L}\p{M}\p{N}._@+ -]+\/)*\.claude\/decisions\/[\p{L}\p{M}\p{N}._-]+\/?$/u;
export function parseDir(dir) {
  if (typeof dir !== "string" || dir.length > SLATE_MAX.dir || !SLATE_DIR.test(dir))
    return null;
  if (dir.split("/").some((seg) => seg === ".." || seg === "."))
    return null;
  return dir.replace(/\/$/, "");
}
export function slateName(dir) {
  const parts = dir.split(/[\\/]+/).filter((p) => p !== "");
  return parts[parts.length - 1] ?? "pane";
}
const blank = () => ({ choice: null, choices: [], notes: "", response: "", deferred: false });

export class Slate {
  spec;
  dir;
  picks = new Map;
  at = 0;
  notes = "";
  constructor(spec, dir) {
    this.spec = spec;
    this.dir = dir;
    for (const d of spec.decisions)
      this.picks.set(d.id, blank());
  }
  get total() {
    return this.spec.decisions.length;
  }
  get onReview() {
    return this.at >= this.total;
  }
  card() {
    return this.spec.decisions[this.at];
  }
  pick(id) {
    let p = this.picks.get(id);
    if (p === undefined) {
      p = blank();
      this.picks.set(id, p);
    }
    return p;
  }
  go(step) {
    this.at = Math.max(0, Math.min(this.total, this.at + step));
  }
  goTo(index) {
    this.at = Math.max(0, Math.min(this.total, index));
  }
  isDecided(d) {
    const p = this.pick(d.id);
    if (d.type === "text")
      return p.response.trim() !== "";
    return d.multi ? p.choices.length > 0 : p.choice !== null;
  }
  isSkipped(d) {
    return this.pick(d.id).deferred && !this.isDecided(d);
  }
  choose(d, optionId) {
    const p = this.pick(d.id);
    if (d.multi) {
      p.choices = p.choices.includes(optionId) ? p.choices.filter((c) => c !== optionId) : [...p.choices, optionId];
    } else {
      p.choice = p.choice === optionId ? null : optionId;
    }
    if (this.isDecided(d))
      p.deferred = false;
  }
  canSkip(d) {
    return !this.isDecided(d) || d.type !== "text" && !d.multi;
  }
  skip(d) {
    const p = this.pick(d.id);
    if (p.deferred) {
      p.deferred = false;
      return;
    }
    if (!this.canSkip(d))
      return;
    p.choice = null;
    p.choices = [];
    p.response = "";
    p.deferred = true;
  }
  setNotes(d, text) {
    this.pick(d.id).notes = cut(text);
  }
  setResponse(d, text) {
    const p = this.pick(d.id);
    p.response = cut(text);
    if (this.isDecided(d))
      p.deferred = false;
  }
  setSlateNotes(text) {
    this.notes = cut(text);
  }
  acceptRecommended() {
    let n = 0;
    for (const d of this.spec.decisions) {
      if (d.type === "text" || this.isDecided(d) || this.pick(d.id).deferred)
        continue;
      const rec = d.options.filter((o) => o.recommended).map((o) => o.id);
      if (rec.length === 0)
        continue;
      const p = this.pick(d.id);
      if (d.multi)
        p.choices = rec;
      else
        p.choice = rec[0] ?? null;
      n++;
    }
    return n;
  }
  counts() {
    let decided = 0;
    let skipped = 0;
    for (const d of this.spec.decisions) {
      if (this.isDecided(d))
        decided++;
      else if (this.isSkipped(d))
        skipped++;
    }
    return { decided, skipped, open: this.total - decided - skipped };
  }
  summary(d) {
    const p = this.pick(d.id);
    if (d.type === "text")
      return clean(p.response.split(`
`)[0], SLATE_MAX.label);
    const label = (id) => id === OTHER_ID ? OTHER_LABEL : d.options.find((o) => o.id === id)?.label ?? id;
    return d.multi ? p.choices.map(label).join(", ") : p.choice === null ? "" : label(p.choice);
  }
  /** The person's work so far, to keep across a reload of the pane. */
  draft() {
    return { at: this.at, notes: this.notes, picks: Object.fromEntries(this.picks) };
  }
  /** Puts back a saved draft, keeping only what still fits this slate. */
  restore(saved) {
    if (!isObj(saved)) return;
    if (Number.isInteger(saved.at)) this.goTo(saved.at);
    if (typeof saved.notes === "string") this.setSlateNotes(saved.notes);
    const picks = isObj(saved.picks) ? saved.picks : {};
    for (const d of this.spec.decisions) {
      const p = picks[d.id];
      if (!isObj(p)) continue;
      const ids = new Set([...d.options.map((o) => o.id), OTHER_ID]);
      const mine = this.pick(d.id);
      if (d.type !== "text") {
        if (d.multi && Array.isArray(p.choices)) mine.choices = p.choices.filter((c) => ids.has(c));
        if (!d.multi && typeof p.choice === "string" && ids.has(p.choice)) mine.choice = p.choice;
      }
      if (typeof p.notes === "string") mine.notes = cut(p.notes);
      if (typeof p.response === "string" && d.type === "text") mine.response = cut(p.response);
      mine.deferred = p.deferred === true && !this.isDecided(d);
    }
  }
  response(now = new Date) {
    return {
      slate: slateName(this.dir),
      mode: this.spec.mode,
      exportedAt: now.toISOString(),
      notes: clean(this.notes, SLATE_MAX.notes, true),
      attachments: [],
      decisions: this.spec.decisions.map((d) => {
        const p = this.pick(d.id);
        const entry = { id: d.id, issues: [...d.issues], title: d.title, type: d.type, deferred: this.isSkipped(d) };
        if (d.type === "text") {
          entry["response"] = clean(p.response, SLATE_MAX.notes, true);
          return entry;
        }
        const labelFor = (id) => id === OTHER_ID ? OTHER_LABEL : d.options.find((o) => o.id === id)?.label ?? null;
        entry["notes"] = clean(p.notes, SLATE_MAX.notes, true);
        if (d.type === "options")
          entry["select"] = d.multi ? "multi" : "single";
        if (d.multi) {
          entry["choices"] = [...p.choices];
          entry["choiceLabels"] = p.choices.map(labelFor);
        } else {
          const opt = p.choice === null || p.choice === OTHER_ID ? undefined : d.options.find((o) => o.id === p.choice);
          entry["choice"] = p.choice;
          entry["choiceLabel"] = p.choice === null ? null : labelFor(p.choice);
          entry["isRecommended"] = p.choice === OTHER_ID ? false : opt === undefined ? null : opt.recommended;
        }
        return entry;
      })
    };
  }
}
