//! FORK(attention): the window's part of the attention queue
//! (`crate::fork::attention`, cameronsjo/herdr-app#16). `src/window.rs`
//! carries only one-line `FORK(attention)` call sites.

use super::*;
use crate::fork::attention;

impl AppWindow {
    /// Every tick: removes the attention label from agents that work again,
    /// and reads working agents' screens for the stall check.
    pub(super) fn fork_attention_tick(&mut self, cx: &mut Context<Self>) {
        for addr in attention::labels_to_clear(&self.live) {
            let core = self.core.clone();
            cx.spawn(async move |_, cx| {
                let params = json!({ "terminal": addr, "labels": { (attention::LABEL): "" } });
                let Err(e) = cx.background_executor().spawn(async move { core.call("terminal.label", params) }).await else { return };
                tracing::warn!("attention label for {addr}: {e:#}");
                attention::cleared_failed(&addr);
            })
            .detach();
        }
        let now = model::now_ms();
        let limit = attention::stall_limit_ms(now);
        for addr in attention::screens_to_read(&self.live, now, limit) {
            let core = self.core.clone();
            cx.spawn(async move |this, cx| {
                let params = json!({ "terminal": addr, "lines": 0 });
                let read = cx.background_executor().spawn(async move { core.call("terminal.read", params) }).await;
                let Ok(v) = read else { return };
                let Some(text) = v.get("text").and_then(Value::as_str) else { return };
                attention::saw_screen(&addr, text, model::now_ms());
                // A stall that starts or ends changes what the mods show.
                let _ = this.update(cx, |this, cx| this.publish(cx));
            })
            .detach();
        }
    }

    /// The attention queue's facts on one entry of the mods' `agents()`:
    /// `reason` (why it needs you), `attention` (the label's text) and
    /// `stalled`.
    pub(super) fn fork_agent_fields(&self, agent: Option<&mut Value>, addr: &str) {
        let (Some(agent), Some(rec)) = (agent, self.live.get(addr)) else { return };
        let now = model::now_ms();
        agent["reason"] = attention::reason(rec).map_or(Value::Null, Value::from);
        agent["attention"] = attention::attention(rec).map_or(Value::Null, Value::from);
        agent["stalled"] = Value::Bool(rec.get("status").and_then(Value::as_str) == Some("working") && attention::stalled(addr, now, attention::stall_limit_ms(now)));
    }
}
