# The local patch on Rune

The Rune adapters build against anomalyco/rune's `dev` at
`998bef587f09e359d877b35c66e48b8f355e73cb` with this one patch
(`rune-wheel.patch`, which `build.sh` applies to its checkout). It is not
offered upstream: we only consume Rune.

**Why.** The vlist scene moves its list by pixels each tick (a steady scroll,
flings). The other adapters set their list's offset through their
framework's API. Rune's public API has no vertical scroll offset: `Tx` has
`scroll_into_view` (as little as shows a row), `scroll_page` (90% pages),
`scroll_to_bottom` and the horizontal `scroll_to_x`. A vertical offset moves
only with the user's wheel, which the window hands to `Runtime::wheel_xy`,
and in headless scripts.

**What.** `Tx::wheel(x, y, dy)` queues a wheel delta that Rune hands to that
same `Runtime::wheel_xy`, as the window does a `MouseWheel` event at the
pointer. From `AppHandle::update` (what the adapters use: their clock is a
timer thread) it is delivered at once, as the window delivers a wheel event
when it arrives, between frames; from the app's `poll`, right after the poll,
before the frame's layout; from a handler, with the frame's other scroll
requests. Nothing else changes: no other code path, no optimization, no
behaviour of an app that doesn't call it. `dy` is in points; positive moves
the content down (scrolls up), as `Runtime::wheel` takes it.

```diff
diff --git a/crates/rune/src/el.rs b/crates/rune/src/el.rs
index f981352..9f13476 100644
--- a/crates/rune/src/el.rs
+++ b/crates/rune/src/el.rs
@@ -37,6 +37,8 @@ pub struct Tx {
     pub(crate) scroll_pages: Vec<(String, i32)>,
     /// Horizontal scroll viewports to move: (key, offset in points).
     pub(crate) scroll_x: Vec<(String, f32)>,
+    /// Wheel deltas to deliver as the platform does: (x, y, dy) in points.
+    pub(crate) wheel: Vec<(f32, f32, f32)>,
     pub(crate) wakes: Vec<std::time::Duration>,
     pub(crate) shell: Option<Shell>,
     pub(crate) minimize: bool,
@@ -85,7 +87,7 @@ impl Tx {
     /// or a text flight. A `wake_after` only schedules.
     pub(crate) fn frame_work(&self) -> bool {
         !self.scroll_to.is_empty() || !self.scroll_bottom.is_empty() || !self.scroll_pages.is_empty() || !self.scroll_x.is_empty()
-            || !self.text_flights.is_empty()
+            || !self.wheel.is_empty() || !self.text_flights.is_empty()
     }
 
     /// An app marker on the next frame, e.g. `tx.trace_event(format_args!("lab:{step:?}"))`.
@@ -131,6 +133,13 @@ impl Tx {
     pub fn scroll_to_x(&mut self, key: impl Into<String>, x: f32) {
         self.scroll_x.push((key.into(), x));
     }
+    /// Deliver a vertical wheel delta of `dy` points at `x, y` in the window, through the same
+    /// `Runtime::wheel_xy` a platform wheel event takes (positive `dy` moves content down):
+    /// from `AppHandle::update` at once, as the window delivers a wheel event; from `poll`
+    /// before the frame's layout; from a handler with the frame's other scroll requests.
+    pub fn wheel(&mut self, x: f32, y: f32, dy: f32) {
+        self.wheel.push((x, y, dy));
+    }
     /// Wake the app's poll hook after an unscaled duration. Equal deadlines coalesce.
     /// This schedules a frame, not a view commit; poll returns true if state changed.
     /// For debounce, keep the latest due time in app state and reschedule the remaining
diff --git a/crates/rune/src/runtime.rs b/crates/rune/src/runtime.rs
index b46705f..b1c0606 100644
--- a/crates/rune/src/runtime.rs
+++ b/crates/rune/src/runtime.rs
@@ -1417,6 +1417,7 @@ impl<S: 'static> Runtime<S> {
         let mut one = self.transaction();
         let changed = f(&mut self.state, &mut one);
         let stale = changed || one.frame_work() || one.window_frame.is_some();
+        for (x, y, dy) in std::mem::take(&mut one.wheel) { self.wheel_xy(x, y, 0.0, dy); }
         self.latch_requests(&mut one);
         merge(&mut self.pending, one);
         self.dirty |= changed;
@@ -1747,6 +1748,7 @@ impl<S: 'static> Runtime<S> {
                 ran = true;
             }
             merge(&mut tx, one);
+            for (x, y, dy) in std::mem::take(&mut tx.wheel) { self.wheel_xy(x, y, 0.0, dy); }
         }
         if self.commits == 0 && self.queue.iter().any(|a| matches!(a, Action::WindowFocus(_))) {
             self.commit(&Tx::default());
@@ -1818,6 +1820,7 @@ impl<S: 'static> Runtime<S> {
         if !scroll_bottom.is_empty() { self.scroll_to_bottom(scroll_bottom); }
         if !scroll_pages.is_empty() { self.scroll_pages(scroll_pages); }
         if !scroll_x.is_empty() { self.scroll_to_x(scroll_x); }
+        for (x, y, dy) in std::mem::take(&mut tx.wheel) { self.wheel_xy(x, y, 0.0, dy); }
         // Those scrolls moved the caret after the commit a view placed from it saw.
         if self.caret_watch && self.caret_moved(None) { self.commit(&Tx { instant: true, ..Tx::default() }); }
         self.update_interaction();
@@ -5470,6 +5473,7 @@ fn merge(tx: &mut Tx, one: Tx) {
     tx.scroll_bottom.extend(one.scroll_bottom);
     tx.scroll_pages.extend(one.scroll_pages);
     tx.scroll_x.extend(one.scroll_x);
+    tx.wheel.extend(one.wheel);
     tx.wakes.extend(one.wakes);
     tx.text_flights.extend(one.text_flights);
     tx.popups.extend(one.popups);
```
