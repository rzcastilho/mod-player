# Contract: The Panel Card and its Persisted State

**Feature**: 016 | Covers FR-017–FR-023, FR-024, FR-035–FR-037

Two halves: the shared card helper (`C*`) and the `settings.toml`-backed
open/closed flags (`P*`).

---

## The card helper (`widgets::controls::panel_card`)

**C1 — One implementation, four call sites.** The fill, padding, radius,
uppercase header and accessible-name pin exist exactly once, in
`crates/modplayer-ui/src/widgets/`. All four panels render through it.
*Test*: source-level assertion — no `surface_raised` fill with an
`inner_margin` outside `widgets/controls.rs`; plus the four panel tests
each asserting the same card values. (FR-036, Clarification 14)

**C2 — The card's values are existing tokens.** Fill
`roles.surface_raised`; padding `theme::space::LG` on all four sides;
corner radius `theme::radius::MD`; **no stroke**. *Test*:
`widgets/controls.rs` unit, both themes. (FR-017, FR-024)

**C3 — Every card shows a `section`-role uppercase header.** Through
`theme::section_label`. *Test*: `widgets/controls.rs` unit +
`tests/now_playing.rs` for all four panels. (FR-018)

**C4 — Each header's accessible name is the exact, un-uppercased
string.** Pinned via `accesskit_node_builder`, mirroring
`markers.rs:545-548`. *Test*: `tests/accessibility.rs` — the existing
Markers and Effect Chain heading-name assertions must pass
**unmodified**, and the two new headers (Transport, Queue) assert the
same shape. (FR-018, FR-025)

**C5 — The four panels are Markers, Effect Chain, Transport, Queue.**
`markers::panel`, `effects_view::show`, `transport_view::show`,
`queue_view::show`. *Test*: `tests/now_playing.rs` — four card nodes in
a frame with all three panels open. (FR-017)

**C6 — No panel renders its header twice.** Markers
(`markers.rs:544-548`) and Effect Chain (`effects_view.rs:99-103`)
already draw a `section_label` inside their own first `ui.horizontal`;
those lines are **deleted** when the panel is wrapped. Transport's
`ui.heading()` (`transport_view.rs:48`) is deleted and replaced by the
helper's. *Test*: `tests/now_playing.rs` — exactly one `Role::Heading`
node per panel, with the expected name. (FR-018, research R7)

**C7 — Transport's header becomes `section`, not `title`.** The one
panel that did not use the shared treatment now does. *Test*:
`tests/transport_view.rs` / `tests/accessibility.rs`. (FR-018)

**C8 — Queue gains a header reading "Queue".** New Fluent key
`queue-panel-title` in `playback.ftl`. *Test*: `tests/queue_view.rs` +
`tests/fluent_keys.rs`. (FR-018)

**C9 — No collapse control is added to any card header.** The three
control-row switches (`now_playing.rs:145-159`) and the `Q`/`E`/`T`
shortcuts remain the only affordance. *Test*: `tests/now_playing.rs` —
the interactive-node count inside each card is unchanged from today.
(FR-035, Clarification 13)

**C10 — A collapsed panel draws nothing at all.** Not a header-only
card. The `if open` guards at `now_playing.rs:174, 194, 207` are kept.
*Test*: `tests/now_playing.rs` — a closed panel contributes no card and
no heading node. (FR-035)

**C11 — Markers gets the card but no toggle.** It has none today.
*Test*: `tests/markers.rs` — card present, interactive-node count
unchanged. (FR-021)

**C12 — Wrapping changes chrome only.** Every existing click target,
keyboard shortcut and datum inside every panel behaves exactly as today.
*Test*: `tests/markers.rs`, `tests/effects_view.rs`,
`tests/transport_view.rs`, `tests/queue_view.rs` pass unmodified except
for FR-037's two seed helpers. (FR-022)

**C13 — `EFFECTS_PANEL_RESERVED_HEIGHT` is re-derived from tokens.** No
pixel literal survives; the value is a sum of `theme::space` tokens,
measured widget heights, and `2 × space::LG` per card now sitting in the
reserved region. It must be **at least** the old 140.0 plus the new card
insets, so the 2026-09-19 clipping defect cannot recur. *Test*:
`tests/now_playing.rs` — at a 960×640 viewport with all three panels
open, the master-volume row, the peak meter and the Queue card all have
rects inside the viewport. (FR-023, FR-024, Edge Case)

---

## The persisted flags

**P1 — `settings.toml` is the single source of truth.** The three
`ui.memory` flags and their `panel_open_id` helpers
(`now_playing.rs:44-46`, `effects_view.rs:36-38`,
`transport_view.rs:22-24`) are **removed**, not mirrored. *Test*:
source-level assertion — no `get_temp::<bool>` / `insert_temp` for a
panel id anywhere in `crates/modplayer-ui/src`. (FR-019)

**P2 — The UI reaches persistence through a public controller pair.**
`now_playing_panel_open(panel)` / `set_now_playing_panel_open(panel,
open)`, shaped like `focus_policy`/`set_focus_policy`
(`controller.rs:2448-2458`). `persist_settings` stays private. *Test*:
`controller.rs` unit — set, reload the store, read back. (FR-019,
Clarification 15)

**P3 — Every toggle persists immediately, from either input path.** The
control-row switch and the `Q`/`E`/`T` shortcut write through the same
setter, so a click and a shortcut cannot diverge. *Test*:
`tests/now_playing.rs` — toggle via the switch, assert persisted; invoke
`HostAction::ToggleQueue` / `ToggleEffectChain` /
`ToggleTransportPanel`, assert persisted. (FR-019, US3 Scenario 5)

**P4 — The section is optional and absent-defaults-closed.** A
`settings.toml` with no `[now_playing_panels]` loads all three panels
closed — the same default the `ui.memory` lookups use today. *Test*:
`crates/modplayer-core/src/settings/model.rs` unit, mirroring
`plugin_panels_round_trip`. (FR-020, Edge Case)

**P5 — `SCHEMA_VERSION` stays `1`.** An absent optional table is not a
schema change — `[onboarding]`'s precedent. *Test*: the existing
schema-version assertion (`model.rs:813`) passes unmodified. (FR-020)

**P6 — The three panels persist and restore independently.** Changing
one leaves the other two's stored values untouched, in both directions.
*Test*: `model.rs` unit over all three, collapse-then-restore and
expand-then-restore. (FR-019, US3 Scenarios 1–3, SC-007)

**P7 — The three toggle functions take the controller, not the
context.** `actions.rs:518, 531, 532` pass `controller`. **`invoke`'s own
signature is unchanged** — it already holds
`controller: &mut PlaybackController<B, H>`. *Test*: compilation, plus
P3's shortcut assertions. (FR-019, research R9)

**P8 — The two memory-seeding test helpers are migrated.**
`tests/now_playing.rs` and `tests/effects_view.rs` seed the persisted
value instead of the egui memory id. This is the **one** explicit
relaxation of FR-025; no other existing assertion may be edited. *Test*:
the diff itself — only those two helpers change. (FR-037, SC-009)

**P9 — `[now_playing_panels]` round-trips for *any* combination of the
three flags.** Constitution VIII requires a property-based test for new
serialized state, not only the example-based cases P4/P6 pin. For an
arbitrary `(effect_chain_open, transport_open, queue_open)` triple, a real
save/load returns the same three values with no warning, and
`SCHEMA_VERSION` is still `1`. *Test*: a `proptest!` block in
`crates/modplayer-core/tests/settings.rs`, beside — and shaped like —
the existing `plugin_panels_round_trip_proptest` (`:682-710`), which is
the crate's established precedent for this principle. (FR-019, FR-020,
Constitution VIII, SC-007)
