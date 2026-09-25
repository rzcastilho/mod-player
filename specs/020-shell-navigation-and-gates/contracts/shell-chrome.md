# Contract: Shell Chrome — Rail Visibility, Step Indicator, Rail Selected State

**Covers**: FR-001, FR-001a, FR-001b, FR-002, FR-003, FR-004, FR-005, FR-006 · SC-001, SC-005
**Module**: `crates/modplayer-ui/src/shell.rs`, `crates/modplayer-ui/src/widgets/controls.rs`,
`crates/modplayer-ui/src/theme/controls.rs`, wiring in `crates/modplayer-ui/src/app.rs`
**Tests**: `crates/modplayer-ui/tests/shell_navigation.rs` (new), `shell.rs` unit tests,
`tests/accessibility.rs`, `tests/design_token_contrast.rs`

## Public surface (crate-internal `pub`, re-exported through `modplayer_ui::shell` for tests)

```rust
pub enum GateStep { Welcome, SignIn, AudioOutputCheck }
impl GateStep {
    pub const ALL: [GateStep; 3];
    pub fn position(self) -> u8;                     // 1..=3
    pub fn label_key(self) -> &'static str;
    pub fn from_launch_step(step: LaunchStep) -> Option<GateStep>;
    pub fn state_relative_to(self, current: GateStep) -> StepState;
}
pub const GATE_STEP_TOTAL: u8 = 3;
pub enum StepState { Complete, Current, Upcoming }

pub struct Chrome { pub rail: bool, pub gate: Option<GateStep> }
impl Chrome {
    pub fn for_frame(step: LaunchStep, device_check_open: bool) -> Chrome;
    pub fn navigation_enabled(self) -> bool;
}

/// Adds `Panel::left("shell-nav-rail")` iff `chrome.rail`, and
/// `Panel::top("gate-step-indicator")` iff `chrome.gate.is_some()`.
pub fn show_chrome(ui: &mut egui::Ui, chrome: Chrome, shell: &mut Shell);

// widgets::controls
pub fn nav_item(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response;
// theme::controls
pub fn nav_indicator(roles: &Roles) -> egui::Stroke;   // accent, 3.0 / 4.0 (high contrast)
```

## Clauses

| # | Clause | Verified by |
|---|---|---|
| C1 | `Chrome::for_frame` matches research.md R1's table for all 4 × 2 inputs; `rail ⇒ gate == None`. | unit, exhaustive |
| C2 | With `chrome.rail == false`, the AccessKit tree of a `show_chrome` frame contains **no** node labelled any `tr("nav-*")`, and no `Panel` with id `shell-nav-rail` exists in egui memory's area/panel state; the central content rect's left edge == screen left (± 0.5 px). | `shell_navigation.rs` |
| C3 | `App::ui` gates `actions::dispatch` on `Chrome::navigation_enabled()` from the pre-account-tick step; pressing `Cmd/Ctrl+1..5` during any gate frame, then rendering a `Main` frame with no input, leaves `shell.section` unchanged (no deferral). | `shell_navigation.rs` (drives `actions::dispatch_and_invoke` behind the same predicate) + `tests/actions.rs` regression |
| C4 | During a gate, the step indicator is one `Role::ProgressIndicator` node named `"Step {n} of 3: {label}"` with `numeric_value = n`, `max_numeric_value = 3`; `n = GateStep::from_launch_step(step).position()`. Items `< n` paint Complete, `== n` Current, `> n` Upcoming. The indicator contributes **no** focusable node. | `shell_navigation.rs` for each of Welcome/SignIn/DeviceCheck, plus a re-render after a retry sub-state (Authorizing→SignedOut with note, Welcome→Privacy sub-view) asserting `n` unchanged |
| C5 | With `chrome.rail == true`, the rail shows exactly five `nav_item`s in `SECTIONS` order (Library, Search, Now Playing, Plugins, Settings) and nothing else (no count badges). | `shell_navigation.rs` |
| C6 | Selected `nav_item`: no filled rect behind the label (the painted shapes under the item rect contain no `RectShape` with non-transparent fill other than the hover fill, which is absent when not hovered); one vertical line segment at `rect.left()` spanning `rect.y_range()` with `nav_indicator(roles)`; label colour `text_primary`. Unselected: no indicator, label `text_secondary`. | `shell_navigation.rs` (inspect `FullOutput.shapes`) |
| C7 | `nav_indicator(roles).color` vs `roles.surface_base` ≥ 3.0:1 for `LIGHT`, `DARK`, and both high-contrast role sets. | `design_token_contrast.rs` |
| C8 | Selected item AccessKit node: `is_selected() == Some(true)`; every item's label == exact `tr(key)` (not uppercased). | `accessibility.rs` / `shell.rs` unit (existing 5-button name test keeps passing) |
| C9 | Settings-triggered Device Check preview: `Chrome { rail: false, gate: None }` — no rail, no indicator; closing the preview restores the rail the next frame. | unit (C1) + manual M5 |
| C10 | No colour, width or spacing literal in `shell.rs`/`nav_item`; values come from `theme::{tokens,controls}` (`design_token_literals.rs` keeps passing). | existing literal scan |

## Frame ordering in `App::ui` (normative)

```text
apply_tokens → controller.tick → window-size bookkeeping
→ claims = claims_snapshot; clear_claims
→ pre = Chrome::for_frame(launch_step(), device_check.is_some())
→ if pre.navigation_enabled(): dispatch + invoke
→ account.tick() → handle events
→ step = launch_step(); chrome = Chrome::for_frame(step, device_check.is_some())
→ shell::show_chrome(ui, chrome, &mut shell)          // left rail and/or top indicator
→ notifications Area (unchanged, 019)
→ CentralPanel: match step { … }                       // same `step`, no re-evaluation
→ section_memory.end_frame(..) → paint_focus_ring
```
