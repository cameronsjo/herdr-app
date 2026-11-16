// FORK(decisions): the decisions pane. Shows a cadence:ask-questions slate as cards inside
// Herdr, in place of the browser dashboard: pick options (one or many), compare candidates,
// type answers and notes, review, and send them all at once.
//
// Opened beside Claude's terminal by the skill's serve-decisions.py:
//   herdr-app do pane.split '{"direction":"right","mod":"decisions","props":{"dir":"<slate folder>"}}'
// It reads <dir>/slate.json. Send writes <dir>/response.json, the same file the dashboard's
// server writes; the script sees it, appends the ledger line and closes the pane.
// "Close without sending" writes <dir>/closed.json instead. Keys (when no field has the
// keyboard): a-h pick, n or right next, p or left back, s skip, r take the recommended
// options on the review page. Send has no key, so a stray letter never sends.
import { View, div } from "gpui-kit";
import { v_flex, h_flex, InputState, Input, TextareaState, Textarea } from "gpui-base";
import { props, readTextFile, writeTextFile, nextCommand, theme, settings } from "herdr";
import { Slate, parseSlate, parseDir, OTHER_ID, OTHER_LABEL } from "./slate.js";

const LETTERS = "abcdefgh";
const draftKey = (dir) => `draft:${dir}`;

export default class Decisions extends View {
  init(_props, cx) {
    this.handle = cx.focus_handle();
    this.typing = 0;
    this.slate = null;
    this.error = null;
    this.saving = false;
    const raw = props()?.dir;
    this.dir = parseDir(raw);
    // No folder: an empty pane that says how one opens. A folder that isn't a slate folder
    // (an absolute path ending in .claude/decisions/<slug>) is refused before any read.
    this.phase = raw === undefined || raw === null ? "empty" : this.dir === null ? "error" : "loading";
    if (this.phase === "error") this.error = "This pane was opened on a folder that isn't a slate folder.";

    // One field of each kind, reused across cards: a page change loads that card's text into it.
    this.notes = InputState.new({ placeholder: "Anything Claude should know about this pick" });
    this.answer = TextareaState.new({ placeholder: "Type your answer", rows: 4 });
    this.answer.set_auto_grow(3, 12);
    this.extra = TextareaState.new({ placeholder: "Anything else, about the slate as a whole", rows: 3 });
    this.extra.set_auto_grow(2, 10);
    for (const [field, write] of [
      [this.notes, (s, d, v) => s.setNotes(d, v)],
      [this.answer, (s, d, v) => s.setResponse(d, v)],
      [this.extra, (s, _d, v) => s.setSlateNotes(v)],
    ]) {
      field.on("change", (_e, cx) => {
        const s = this.slate;
        if (!s || this.loadingFields) return;
        const d = s.card();
        if (field !== this.extra && !d) return;
        write(s, d, field.value());
        this.saveDraft();
        cx.notify();
      });
      field.on("focus", (_e, cx) => { this.typing++; cx.notify(); });
      field.on("blur", (_e, cx) => { this.typing = Math.max(0, this.typing - 1); cx.notify(); });
    }

    if (this.phase === "loading") cx.spawn(async (cx) => { await this.load(); cx.notify(); });
    // "focusable" in mod.json: pane.go lands here and sends "focus".
    cx.spawn(async (cx) => {
      for (;;) {
        const c = await nextCommand();
        if (c === "focus") this.handle.focus();
        cx.notify();
      }
    });
  }

  async load() {
    try {
      const [spec, sent] = await Promise.all([readTextFile(`${this.dir}/slate.json`), readTextFile(`${this.dir}/response.json`)]);
      if (spec === null) return this.fail("There's no slate.json in this pane's folder.");
      let json;
      try { json = JSON.parse(spec); } catch { return this.fail("slate.json isn't valid JSON."); }
      const parsed = parseSlate(json);
      if (!parsed.ok) return this.fail(`The slate can't be shown: ${parsed.error}.`);
      this.slate = new Slate(parsed.spec, this.dir);
      if (sent !== null) { this.phase = "sent"; return; }
      try { this.slate.restore(JSON.parse(localStorage.getItem(draftKey(this.dir)) ?? "null")); } catch { /* no draft */ }
      this.phase = "open";
      this.loadFields();
    } catch (err) {
      this.fail(`The slate couldn't be read: ${err?.message ?? err}.`);
    }
  }

  fail(message) {
    this.phase = "error";
    this.error = message;
  }

  saveDraft() {
    try { localStorage.setItem(draftKey(this.dir), JSON.stringify(this.slate.draft())); } catch { /* storage is a convenience */ }
  }

  /** Puts the shown card's text into the reused fields, without counting it as an edit. */
  loadFields() {
    const s = this.slate, d = s?.card();
    this.loadingFields = true;
    try {
      if (d) {
        const p = s.pick(d.id);
        this.notes.set_value(p.notes);
        this.answer.set_value(p.response);
      }
      this.extra.set_value(s?.notes ?? "");
    } finally { this.loadingFields = false; }
  }

  move(fn, cx) {
    if (!this.slate || this.phase !== "open") return;
    fn(this.slate);
    this.loadFields();
    this.saveDraft();
    this.handle.focus();
    cx?.notify();
  }

  act(fn, cx) {
    if (!this.slate || this.phase !== "open") return;
    fn(this.slate);
    this.saveDraft();
    cx?.notify();
  }

  async send(cx) {
    const s = this.slate;
    if (!s || this.phase !== "open" || this.saving) return;
    this.saving = true;
    this.error = null;
    cx.notify();
    try {
      // null: only create it. A response.json already there is never replaced.
      await writeTextFile(`${this.dir}/response.json`, `${JSON.stringify(s.response(), null, 2)}\n`, null);
      this.phase = "sent";
      try { localStorage.removeItem(draftKey(this.dir)); } catch { /* storage is a convenience */ }
    } catch (err) {
      this.error = `Couldn't save your answers: ${err?.message ?? err}. Nothing was sent.`;
    } finally {
      this.saving = false;
      cx.notify();
    }
  }

  async setAside(cx) {
    if (!this.slate || this.phase !== "open" || this.saving) return;
    this.saving = true;
    cx.notify();
    try {
      await writeTextFile(`${this.dir}/closed.json`, `${JSON.stringify({ closedAt: new Date().toISOString() })}\n`, null);
    } catch { /* already marked closed */ }
    this.phase = "closed";
    this.saving = false;
    cx.notify();
  }

  key(e, cx) {
    if (this.typing > 0 || this.phase !== "open" || e.modifiers?.control || e.modifiers?.platform || e.modifiers?.alt) return;
    const s = this.slate, d = s.card(), k = e.key;
    if (k === "n" || k === "right") this.move((s) => s.go(1), cx);
    else if (k === "p" || k === "left") this.move((s) => s.go(-1), cx);
    else if (k === "s" && d) this.act((s) => s.skip(d), cx);
    else if (k === "r" && s.onReview) this.act((s) => s.acceptRecommended(), cx);
    else if (d && d.type !== "text" && LETTERS.includes(k) && k.length === 1) {
      const ids = [...d.options.map((o) => o.id), OTHER_ID];
      const id = ids[LETTERS.indexOf(k)];
      if (id === undefined) return;
      this.act((s) => s.choose(d, id), cx);
    } else return;
    cx.stop_propagation();
  }

  render() {
    const t = theme();
    const root = v_flex().id("decisions").track_focus(this.handle).on_key_down((e, cx) => this.key(e, cx))
      .size_full().min_h(0).bg(t.background).text_color(t.foreground).text_size(13);
    if (this.phase === "empty") return root.child(this.notice(t, "No slate here", "Claude opens this pane when it has several decisions for you at once (cadence:ask-questions). Each card is one choice; you answer them all and send them together."));
    if (this.phase === "loading") return root.child(this.notice(t, "Opening the slate…", ""));
    if (this.phase === "error") return root.child(this.notice(t, "Can't show this slate", this.error ?? ""));
    const s = this.slate;
    if (this.phase === "sent") return root.child(this.notice(t, "Sent", `Claude has your answers to ${s.spec.title}. It reads them as your next message.`));
    if (this.phase === "closed") return root.child(this.notice(t, "Closed without sending", "Claude was told you set this slate aside. Nothing was answered."));
    return root.child(this.header(t, s)).child(this.rail(t, s))
      .child(v_flex().id("decisions-body").flex_1().min_h(0).overflow_y_scrollbar().px(18).py(14).gap(12)
        .child(s.onReview ? this.review(t, s) : this.page(t, s, s.card())))
      .when(Boolean(this.error), (el) => el.child(div().px(18).pb(8).text_size(12).text_color(t.blocked).child(this.error)))
      .child(this.footer(t, s));
  }

  notice(t, title, body) {
    return v_flex().flex_1().p(24).gap(8).justify_center()
      .child(div().text_size(15).font_semibold().child(title))
      .when(Boolean(body), (el) => el.child(div().text_size(12.5).text_color(t.muted).line_height("19px").child(body)));
  }

  header(t, s) {
    const where = s.onReview ? "Review" : `${s.at + 1} of ${s.total}`;
    const mode = s.spec.mode === "continue" ? "Claude keeps working meanwhile" : "Claude waits for these";
    return v_flex().px(18).pt(14).pb(10).gap(3)
      .child(h_flex().items_center().gap(10)
        .child(div().flex_1().text_size(15).font_semibold().truncate().child(s.spec.title))
        .child(div().text_size(11.5).text_color(t.dim).child(where)))
      .child(div().text_size(11.5).text_color(t.dim).child(mode));
  }

  /** One segment per card, then the review page: filled when answered, outlined when skipped. */
  rail(t, s) {
    const segs = s.spec.decisions.map((d, i) => {
      const here = i === s.at;
      const fill = s.isDecided(d) ? t.accent : s.isSkipped(d) ? "transparent" : t.raised;
      return div().id(`rail-${i}`).flex_1().h(6).rounded(3).cursor_pointer()
        .bg(fill).border_1().border_color(here ? t.foreground : s.isSkipped(d) ? t.dim : fill)
        .hover((st) => st.opacity(0.8)).on_click((_e, cx) => this.move((s) => s.goTo(i), cx));
    });
    const review = div().id("rail-review").w(18).h(6).rounded(3).cursor_pointer()
      .bg(s.onReview ? t.foreground : t.raised).hover((st) => st.opacity(0.8))
      .on_click((_e, cx) => this.move((s) => s.goTo(s.total), cx));
    return h_flex().px(18).pb(10).gap(4).items_center().children([...segs, review]);
  }

  block(t, text) {
    return div().w_full().p(10).rounded(7).bg(t.raised).font_family(settings().font ?? "JetBrains Mono").text_size(11.5)
      .line_height("16px").overflow_hidden().child(text);
  }

  page(t, s, d) {
    const p = s.pick(d.id);
    const prose = (text) => div().text_size(12.5).text_color(t.muted).line_height("19px").child(text);
    const kids = [];
    if (d.issues.length) kids.push(h_flex().gap(6).flex_wrap().children(d.issues.map((i) => div().px(6).py(1).rounded(5).bg(t.raised).text_size(11).text_color(t.muted).child(i))));
    kids.push(div().text_size(14).font_semibold().line_height("20px").child(d.title));
    if (s.at === 0 && s.spec.intro) kids.push(prose(s.spec.intro));
    kids.push(prose(d.context));
    for (const f of d.walkthrough) kids.push(v_flex().gap(4).child(div().text_size(11.5).font_medium().text_color(t.dim).child(f.label)).child(this.block(t, f.content)));
    if (d.type === "text") {
      kids.push(this.label(t, "Your answer"),
        Textarea.new(this.answer).w_full().px(10).py(8).rounded(8).border_1().border_color(t.input).bg(t.background).text_size(12.8));
      return v_flex().gap(12).children(kids);
    }
    const chosen = (id) => (d.multi ? p.choices.includes(id) : p.choice === id);
    kids.push(div().text_size(11.5).text_color(t.dim).child(d.multi ? "Pick any that apply" : d.type === "compare" ? "Pick one candidate" : "Pick one"));
    const rows = [...d.options, { id: OTHER_ID, label: OTHER_LABEL, recommended: false, pros: [], cons: [], content: "" }].map((o, i) => {
      const on = chosen(o.id);
      const mark = d.multi ? (on ? "☑" : "☐") : on ? "◉" : "○";
      const lines = [
        h_flex().items_center().gap(8)
          .child(div().w(16).text_color(on ? t.accent : t.dim).child(mark))
          .child(div().flex_1().font_medium().child(o.label))
          .when(o.recommended, (el) => el.child(div().px(6).py(1).rounded(5).bg(t.accent).text_color(t.accent_foreground).text_size(10.5).font_medium().child("Recommended")))
          .child(div().text_size(11).text_color(t.dim).child(LETTERS[i]?.toUpperCase() ?? "")),
        ...(o.content ? [this.block(t, o.content)] : []),
        ...o.pros.map((pro) => div().pl(24).text_size(12).text_color(t.ansi2).child(`+ ${pro}`)),
        ...o.cons.map((con) => div().pl(24).text_size(12).text_color(t.ansi3).child(`− ${con}`)),
      ];
      return v_flex().id(`${d.id}:opt:${o.id}`).w_full().px(12).py(9).gap(5).rounded(8).border_1()
        .border_color(on ? t.accent : t.border).bg(on ? t.selected : t.raised).cursor_pointer()
        .hover((st) => st.border_color(on ? t.accent : t.ring))
        .on_click((_e, cx) => this.act((s) => s.choose(d, o.id), cx))
        .children(lines);
    });
    kids.push(v_flex().gap(6).children(rows),
      this.label(t, p.choice === OTHER_ID || p.choices.includes(OTHER_ID) ? "Notes (say what else you mean)" : "Notes (optional)"),
      Input.new(this.notes).h(34).px(10).rounded(8).border_1().border_color(t.input).bg(t.background).text_size(12.8));
    return v_flex().gap(12).children(kids);
  }

  review(t, s) {
    const { decided, skipped, open } = s.counts();
    const parts = [`${decided} answered`, skipped ? `${skipped} skipped` : "", open ? `${open} still open` : ""].filter(Boolean).join(" · ");
    const rows = s.spec.decisions.map((d, i) => {
      const state = s.isDecided(d) ? "done" : s.isSkipped(d) ? "skipped" : "open";
      return h_flex().id(`review:${d.id}`).w_full().px(12).py(8).gap(10).rounded(8).bg(t.raised).cursor_pointer()
        .hover((st) => st.bg(t.hover)).on_click((_e, cx) => this.move((s) => s.goTo(i), cx))
        .child(div().w(14).text_color(state === "done" ? t.accent : t.dim).child(state === "done" ? "●" : state === "skipped" ? "◌" : "○"))
        .child(v_flex().flex_1().min_w(0).gap(2)
          .child(div().truncate().child(d.title))
          .child(div().text_size(11.5).truncate().text_color(state === "open" ? t.blocked : t.muted)
            .child(state === "done" ? s.summary(d) : state === "skipped" ? "Skipped for now" : "Not answered yet")));
    });
    return v_flex().gap(12)
      .child(div().text_size(14).font_semibold().child("Review and send"))
      .child(div().text_size(12.5).text_color(t.muted).child(`${parts}. Open cards go to Claude as skipped.`))
      .child(v_flex().gap(6).children(rows))
      .child(this.label(t, "Anything else"))
      .child(Textarea.new(this.extra).w_full().px(10).py(8).rounded(8).border_1().border_color(t.input).bg(t.background).text_size(12.8));
  }

  label(t, text) {
    return div().pt(4).text_size(11.5).text_color(t.dim).child(text);
  }

  button(t, id, text, kind, action) {
    const primary = kind === "primary", quiet = kind === "quiet";
    return h_flex().id(id).h(30).px(12).items_center().rounded(8).border_1()
      .border_color(primary ? t.solid : quiet ? "transparent" : t.input).bg(primary ? t.solid : quiet ? "transparent" : t.raised)
      .text_color(primary ? t.solid_foreground : quiet ? t.muted : t.foreground).text_size(12.5).font_medium()
      .when(this.saving, (el) => el.opacity(0.5))
      .cursor_pointer().hover((st) => st.opacity(0.88)).on_click(action).child(text);
  }

  footer(t, s) {
    const bar = h_flex().px(18).py(12).gap(8).items_center().border_t_1().border_color(t.border);
    if (s.onReview) {
      const { decided } = s.counts();
      const hasRecommended = s.spec.decisions.some((d) => d.type !== "text" && !s.isDecided(d) && !s.isSkipped(d) && d.options.some((o) => o.recommended));
      return bar
        .child(this.button(t, "back", "Back", "plain", (_e, cx) => this.move((s) => s.go(-1), cx)))
        .when(hasRecommended, (el) => el.child(this.button(t, "accept", "Take recommended (R)", "plain", (_e, cx) => this.act((s) => s.acceptRecommended(), cx))))
        .child(div().flex_1())
        .child(this.button(t, "set-aside", "Close without sending", "quiet", (_e, cx) => this.setAside(cx)))
        .child(this.button(t, "send", decided === 1 ? "Send 1 answer" : `Send ${decided} answers`, "primary", (_e, cx) => this.send(cx)));
    }
    const d = s.card();
    const last = s.at === s.total - 1;
    return bar
      .when(s.at > 0, (el) => el.child(this.button(t, "back", "Back", "plain", (_e, cx) => this.move((s) => s.go(-1), cx))))
      .child(div().flex_1())
      .when(s.canSkip(d) || s.isSkipped(d), (el) => el.child(this.button(t, "skip", s.isSkipped(d) ? "Skipped: undo" : "Skip for now", "quiet", (_e, cx) => this.act((s) => s.skip(d), cx))))
      .child(this.button(t, "next", last ? "Review" : "Next", "primary", (_e, cx) => this.move((s) => s.go(1), cx)));
  }
}
