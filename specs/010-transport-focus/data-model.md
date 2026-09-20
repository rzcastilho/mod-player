# Data Model: Transport Focus Arbitration

**Feature**: 010-transport-focus | **Date**: 2026-09-19 | **Spec**: [spec.md](spec.md) | **Research**: [research.md](research.md)

Phase 1 output. `+` = new type, `~` = changed type. Rust signatures are
indicative; field visibility follows 009's conventions (private with
accessors unless the UI reads them as a view).

## 1. `modplayer-core` — `src/plugins/focus.rs` (+)

### 1.1 `FocusPolicy` (spec Key Entities "Focus Policy")

```rust
+ #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
+ pub enum FocusPolicy {
+     Manual,
+     #[default]
+     AutoOnInteraction,
+     FirstRequestWins,
+ }
+ impl FocusPolicy {
+     pub const ALL: [FocusPolicy; 3];
+     pub fn wire_name(self) -> &'static str;          // "manual" | "auto_on_interaction" | "first_request_wins"
+     pub fn parse(s: &str) -> Option<FocusPolicy>;    // inverse; None → caller falls back to default
+     pub fn label_key(self) -> &'static str;          // Fluent key for the selector
+ }
```

### 1.2 `FocusHolder`

```rust
+ #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
+ pub enum FocusHolder { #[default] Host, Plugin(PluginId) }
```

### 1.3 `FocusArbiter` (spec Key Entities "Transport Focus State")

```rust
+ pub struct FocusArbiter {
+     policy: FocusPolicy,
+     holder: FocusHolder,
+     pending: Vec<PluginId>,          // FIFO of outstanding, ungranted requests; no duplicates
+     host_locked_for_track: bool,     // first-request-wins only: set by a user "Take back"; cleared on track change
+ }
```

Invariants:
- `holder == Plugin(p)` ⇒ `p ∉ pending` (a grant clears the request, FR-011).
- `pending` has no duplicates (re-request keeps position, FR-011).
- Session-scoped; never serialized (FR-012). `policy` is mirrored from
  `AudioSettings.focus_policy` at launch and on every change.

Inputs (all pure; each returns `Vec<FocusChange>`):

| Method | Meaning (spec) |
|---|---|
| `set_policy(policy) -> ()` | FR-005: never changes `holder`/`pending`; returns nothing |
| `request(p: PluginId, running: bool)` | FR-003: no-op if `holder == Plugin(p)`; else push to `pending` if absent; under `FirstRequestWins`, if `holder == Host && !host_locked_for_track` → grant `p` immediately (a plugin ahead in `pending` is not skipped: the grant goes to `pending.first()`, which is `p` when the queue was empty) |
| `release(p)` | FR-004/FR-011: if `holder == Plugin(p)` → `vacate(p, Voluntary)`; else remove `p` from `pending` (withdrawal); always Ok |
| `give(p)` | FR-008 user "Give focus": if `holder == Plugin(p)` → nothing; else revoke current (if plugin), grant `p`, remove `p` from `pending`; under `FirstRequestWins` clears `host_locked_for_track` (a user grant defines the track's holder) |
| `take_back()` | FR-008 user "Take back": if `holder == Host` → nothing; else revoke to Host; under `FirstRequestWins` set `host_locked_for_track = true` (no refill) |
| `local_host_action()` | FR-005 auto: if `policy == AutoOnInteraction && holder == Plugin(_)` → revoke to Host; otherwise nothing |
| `vacate(p, Vacancy)` | FR-006/FR-007: remove `p` from `pending`; if `holder == Plugin(p)` → holder = Host (a `focus_revoked` is **not** sent for `Fault` — the plugin is gone/suspended; it **is** sent for `Voluntary`); then under `FirstRequestWins` and `!host_locked_for_track` → grant `pending.first()` if any (`Vacancy::Fault \| Voluntary` only; never on `take_back`) |
| `on_track_changed()` | FR-006: only under `FirstRequestWins`: revoke holder to Host (with `focus_revoked`), clear `pending`, `host_locked_for_track = false`; else nothing |
| `remove_plugin(p)` | uninstall/invalid (defensive; no uninstall exists this slice): same as `vacate(p, Fault)` |

Reads: `policy()`, `holder()`, `pending()` (slice, FIFO order),
`request_order(p) -> Option<usize>` (1-based).

### 1.4 `FocusChange` / `Vacancy`

```rust
+ #[derive(Debug, Clone, Copy, PartialEq, Eq)]
+ pub enum FocusChange {
+     Revoked { plugin: PluginId, new_holder: FocusHolder },   // deliver focus_revoked{holder: new_holder} to `plugin`
+     Granted { plugin: PluginId },                            // deliver focus_granted{holder: plugin} to `plugin`
+ }
+ #[derive(Debug, Clone, Copy, PartialEq, Eq)]
+ pub enum Vacancy { Voluntary, Fault }
```

Ordering rule (FR-014): within one returned `Vec`, every `Revoked`
precedes every `Granted`. The controller applies them in order:
`FocusToken::set_holder(..)` first (once, to the final holder), then
sends each event to its plugin's handle (skipped silently when the
handle is gone — the `Fault` case).

### 1.5 `TransportActor` (controller-private, research R3)

```rust
+ #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
+ pub(crate) enum TransportActor { #[default] LocalUser, Plugin(PluginId), Remote }
```

Set by a scoped helper `with_transport_actor(actor, |ctrl| ..)` that
restores the previous value on exit (nested calls are safe).

### 1.6 State-transition table (per policy)

| Event | Manual | Auto on interaction | First request wins |
|---|---|---|---|
| `request(p)` | pending += p | pending += p | if holder = Host ∧ ¬locked ∧ pending empty → grant p; else pending += p |
| user `give(p)` | revoke h → grant p | revoke h → grant p | revoke h → grant p; locked = false |
| user `take_back` | revoke h → Host | revoke h → Host | revoke h → Host; locked = true |
| local host transport action | — | revoke h → Host | — |
| remote-controller command | — | — | — |
| `release(h)` by holder | → Host | → Host | → Host; grant pending.first() if ¬locked |
| `release(p)`, p pending | pending −= p | pending −= p | pending −= p |
| disable / suspend / crash of h | → Host; loop disarmed by `stop` | same | same + grant pending.first() if ¬locked |
| disable / suspend / crash of pending p | pending −= p | same | same |
| `track_changed` | — | — | → Host; pending = ∅; locked = false |
| `set_policy(x)` | holder & pending untouched | same | same |
| app relaunch | Host, ∅, policy restored | same | same |

## 2. `modplayer-core` — view and settings deltas

### 2.1 `TransportFocusView` (`src/plugins/view.rs` ~, FR-008)

```rust
+ #[derive(Debug, Clone, PartialEq)]
+ pub struct FocusRow {
+     pub id: PluginId,
+     pub name: String,                 // manifest name
+     pub holds: bool,
+     pub request_order: Option<usize>, // 1-based among outstanding requests; None = not requesting
+ }
+ #[derive(Debug, Clone, PartialEq)]
+ pub struct TransportFocusView {
+     pub policy: FocusPolicy,
+     pub holder: Option<FocusRow>,     // None = "host"
+     pub rows: Vec<FocusRow>,          // enabled ∧ (Loading|Active) ∧ grants ∋ transport.control; sorted by name
+ }
```

Validation: `holder` (if `Some`) is one of `rows`; a plugin without
`transport.control`, or `Disabled`/`Suspended`/`Draining`/`Invalid`,
never appears (FR-008).

### 2.2 Settings (`src/settings/model.rs` ~, `store.rs` ~; FR-012)

```rust
~ pub struct AudioSettings { …, + pub focus_policy: FocusPolicy }
~ pub struct RawSettings   { …, + #[serde(default)] pub transport: RawTransport }
+ pub struct RawTransport  { #[serde(default = "default_focus_policy")] pub focus_policy: String }  // "auto_on_interaction"
```

On-disk:

```toml
[transport]
focus_policy = "auto_on_interaction"   # | "manual" | "first_request_wins"
```

Unknown/absent → `AutoOnInteraction` (+ `InvalidField::FocusPolicy`
warning on unknown, like `theme`). Never stores holder/queue.

### 2.3 Action catalog (`src/actions/catalog.rs` ~, FR-013)

```rust
~ pub enum HostAction { …, + ToggleTransportPanel }
   id "host.nav.toggle_transport_panel" · label key "action-nav-toggle-transport-panel"
   · category Navigation · kind trigger · scope NowPlaying · repeats_while_held false · default ["T"]
```

### 2.4 `PluginHost` (`src/plugins/host.rs` ~)

```rust
~ pub struct PluginHost { …, + arbiter: FocusArbiter }   // `focus: FocusToken` stays (write-side now only here)
+ pub fn arbiter(&self) -> &FocusArbiter
+ pub fn apply_focus_changes(&mut self, changes: Vec<FocusChange>)   // sets token, sends events to handles
~ pub fn stop(..)   // now: apply_focus_changes(arbiter.vacate(id, Vacancy::Fault)) before the marker/chain teardown
```

### 2.5 `PlaybackController` façade (`src/controller.rs` ~)

| Method | Behaviour |
|---|---|
| `transport_focus_view() -> TransportFocusView` | §2.1 |
| `focus_policy() -> FocusPolicy` / `set_focus_policy(FocusPolicy)` | arbiter + `persist_settings` |
| `focus_give(PluginId)` | ignored unless the id is a `TransportFocusView` row |
| `focus_take_back()` | |
| `pub(crate) focus_request(PluginId)` / `focus_release(PluginId)` | called by `plugins::apply` |
| `pub(crate) note_local_transport_action()` | research R3/R4 |
| `pub(crate) with_transport_actor(..)` | §1.5 |

## 3. `modplayer-capability-gateway` deltas

### 3.1 `api/v1.toml` (~) — API 1.1

```toml
[api_version]
major = 1
minor = 1            # was 0

[[event]]
name = "FocusGranted"
requires = "transport.control"
payload = ["holder"]

[[event]]
name = "FocusRevoked"
requires = "transport.control"
payload = ["holder"]
```

Requests unchanged (`TransportRequestFocus`/`TransportReleaseFocus`
keep `needs_focus = false`, category `transport`).

### 3.2 `HostEvent` (`src/event.rs` ~)

```rust
~ pub enum HostEvent { …, + FocusGranted { holder: OwnerInfo }, + FocusRevoked { holder: OwnerInfo } }
```

`OwnerInfo::Host` ↔ `"host"`, `OwnerInfo::Plugin(identifier)` ↔ the
identifier string; `OwnerInfo::Me` is never used for these two events.

### 3.3 `FocusToken` (`src/focus.rs` ~)

```rust
~ impl FocusToken {
      pub fn holder(&self) -> Option<PluginId>            // unchanged (read side, plugin threads)
+     pub fn set_holder(&self, holder: Option<PluginId>)  // core only
−     pub fn try_acquire(..) / release_if(..) / clear(..) // removed (no callers)
  }
```

### 3.4 `Refusal` (`src/refusal.rs` ~)

`Refusal::focus_held()` removed; `RefusalCode` set unchanged.

## 4. Wire payloads (Lua side)

| Event | Payload |
|---|---|
| `focus_granted` | `{ holder = "<own identifier>" }` |
| `focus_revoked` | `{ holder = "host" \| "<other identifier>" }` |

`api.transport.request_focus()` → `true` always (or a `permission_denied`
/ `rate_limited` refusal as before); `api.transport.release_focus()` →
`true` always.

## 5. Fixture packages (+)

| Identifier | Permissions | Script behaviour |
|---|---|---|
| `org.modplayer.fixture.focus-a` | `playback.observe`, `transport.control`, `markers.read`, `markers.write` | `ready_ack` → `request_focus()`; on `focus_granted` → `seek(1000)`, `markers.create_loop(1000, 3000, {transient=true})`, `arm_loop`; on `track_changed` → `request_focus()` again; logs every `focus_*`/`play_state_changed`/`loop_*` event; **hangs (busy loop) on the third `play_state_changed` it receives** so the manual suspension scenario (quickstart M5b) is drivable from the transport; `debug_probe` returns the log |
| `org.modplayer.fixture.focus-b` | `playback.observe`, `transport.control` | `ready_ack` → `request_focus()`; on `focus_granted` → `seek(2000)`; on `track_changed` → `request_focus()` again; on `play_state_changed` while not holder → `seek(0)` (expected `no_focus`, logged); `debug_probe` returns the log |

## 6. Relationships

```
AudioSettings.focus_policy ──(launch / set_focus_policy)──▶ FocusArbiter.policy
PluginRecord{enabled, lifecycle, grants} ──filter──▶ TransportFocusView.rows
FocusArbiter{holder, pending} ──▶ TransportFocusView{holder, request_order}
FocusArbiter ──FocusChange──▶ PluginHost.apply_focus_changes ──▶ FocusToken.set_holder (read by every Gateway::admit)
                                                                └─▶ PluginHandle.send_event(FocusGranted|FocusRevoked)
PluginHost.stop(id, reason) ──▶ arbiter.vacate(id, Fault) ─▶ TrackMarkers.disarm_if_owned_by (unchanged 009 order)
PlaybackController.dispatch(Input::Play|Pause|Stop|Seek|SkipForward|SkipBack) ──LocalUser──▶ arbiter.local_host_action()
PlaybackController.arm_loop/disarm_loop (Ok) ──LocalUser──▶ arbiter.local_host_action()
fan_out_plugin_playback_events (TrackChanged branch) ──▶ arbiter.on_track_changed()
```
