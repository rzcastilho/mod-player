# Feature Specification: Settings Fields, Placeholders, and Account Summary

**Feature Branch**: `feature/028-settings-fields-and-account`

**Created**: 2026-09-30

**Status**: Draft

**Input**: User description: "Implement the feature specified in specs/autonomous/breakdown/011-browse-and-manage-polish/004-settings-fields-and-account.md (id 004, settings-fields-and-account)."

**Source**: UI/UX review §3.7 Settings (UX-33, UX-34, UX-35, UX-36), §4.4, §5.4.

## Clarifications

### Session 2026-09-30 (clarify reviewer — no human escalation needed)

Sources: constitution v1.1.1 (X: keyboard-operable, accessible names, externalised strings, en-US + pt-BR; VI), breakdown `011-browse-and-manage-polish/004` (intent + scope boundary), UI/UX review §3.7 (UX-33..36), and the current code (`crates/modplayer-ui/src/settings/*`, `modplayer-engine::types`, `modplayer-core::settings_registry`, `modplayer-account::session`, `crates/modplayer/src/main.rs`). Items marked **[default]** are assumed conventional defaults; others are **[derived]** from the cited source.

- Q: Which categories get grouped cards, and what are the groups? → A **[default]**: Every category whose body this feature owns renders its fields inside cards, each card with a small group header (section-label style): **Audio** → "Output" (output device, buffer preset, Test output device) and "Level protection" (limiter ceiling, safe-volume switch, safe-volume cap); **Playback** → "Connect device" (device name) and "Markers" (nudge step); **Appearance** → "Theme" (theme, high contrast); **Language** → "Language" (locale); **Account** → summary card + actions (FR-011). Controls (owned by 011/005), Plugins (027), Developer (debug-only) and About (read-only) keep their current body. Field order inside a group is today's order.
- Q: What does "help indented and lighter" mean testably? → A **[default]**: help text starts at least `space::SM` (8 px logical) to the right of its label's left edge, uses the secondary text style (`text::SECONDARY`) and the theme's weak/secondary text role (no hard-coded colour), and keeps the existing 72-character measure cap.
- Q: Which units, and how are they shown? → A **[derived]** (engine types + review UX-34): limiter ceiling in **dBFS** (e.g. "-1.0 dBFS"), safe-volume cap in **%** (e.g. "50%"), nudge step in **ms** (already), buffer preset keeps its "(~N ms)" latency suffix. The unit is rendered inside the value box of the control itself (slider value field / drag value), not in a separate label. Unit strings come from Fluent (en-US and pt-BR). Number formatting (decimal separator) is unchanged by this feature **[default]**.
- Q: How is the permitted range shown? → A **[derived]** from `CeilingDb::MIN/MAX`, `VolumePercent`, `nudge_step_ms` clamp: a localised secondary-style caption directly under each numeric control stating min and max with unit — ceiling "-6.0 to -0.1 dBFS", cap "0 to 100%", nudge step "1 to 1000 ms". The caption values are read from the same constants the control clamps to (no duplicated literals).
- Q: Which fields offer per-field reset, and what are their defaults? → A **[derived]** from `AudioSettings::default()` / engine `Default` impls; scope **[default]**: buffer preset (Balanced), limiter ceiling (-1.0 dBFS), safe-volume switch (on), safe-volume cap (50%), theme (System), high contrast (off), device name (unset → default name), nudge step (10 ms). **No** reset for: output device (default is environment-dependent; resetting would silently switch the live output), locale (single option this slice), action buttons (Test output device, Re-check, Sign out), Controls bindings (already have per-action reset), Developer fields and plugin-declared fields.
- Q: How does reset look and behave? → A **[default]**: a compact secondary button placed after its control on the same row, visible text "Reset" (localised), accessible name "Reset {field} to default" (mirrors existing `controls-reset-action`). Activating it applies the default through the **same setter/persistence path** a manual change uses (no confirmation — single field, trivially re-changed), takes effect immediately, and the button disappears on the next frame; keyboard focus moves to the field's control. Reset appears whenever current ≠ default, however that came about (including hand-edited settings file), and hides as soon as they are equal again. The safe-volume cap reset is offered even when the safe-volume switch is off.
- Q: What exactly is highlighted from search, for how long, and how? → A **[default]**: selecting a `settings_registry` descriptor result opens its category, scrolls the target field's card row into view, gives the control keyboard focus (existing behaviour), and draws a highlight around the whole field (label + help + control): a ≥ 2 px outline in the theme's accent/focus role plus a subtle fill — the outline is the non-colour cue. The highlight clears after **3 seconds** or on the first subsequent pointer press or key press, whichever comes first. It applies to descriptors in Audio, Playback, Appearance, Language and Account (Re-check / Sign out targets the action button). Controls/Developer descriptors keep today's focus-only behaviour; plugin-field hits ("Plugins › plugin › field") keep today's behaviour. An empty query yields no results and therefore no highlight.
- Q: Offline and Privacy & diagnostics — real settings or "not available yet"? → A **[derived]** (breakdown scope boundary: their settings belong to 003-offline-and-library/001 and 007-operations-and-polish/001; `DESCRIPTORS` has no entries for them): both show a **"not available yet"** state: a group-header-styled title (the category name) and one plain sentence, e.g. "Offline settings aren't available yet. They'll arrive in a later update." (per-category Fluent key). They remain selectable (not disabled) so the statement is reachable. Since they have no descriptors, search never lands on them.
- Q: How is a not-yet-available category marked in the category row? → A **[default]**: a small secondary-style text badge "Coming soon" (localised) after the category label, in the inline row, in the pinned slot, and in the "More" overflow menu. The marker is text (not colour or icon alone), its width counts in the row's partition measurement, and the item's accessible name is "{category}, coming soon". Completed categories carry no marker. Category order is unchanged.
- Q: What does the Account summary contain and what are the fallbacks? → A **[derived]** from `AccountSession`: identity = `display_name` (the service already falls back to `account_id` when the profile name is null); tier = Premium / Free / Unknown; last verified = `last_validated_at`. Layout **[default]**: one card; identity is the prominent line (body-strong), tier and last-verified are secondary label–value lines inside the same card; Re-check (standard button, with its help) and then Sign out (destructive, separated by the divider) sit beneath the card. Fallbacks **[default]**: empty identity → "Name unavailable"; `Tier::Unknown` → "Not verified yet"; `last_validated_at = None` → "Never verified online". Last-verified time is shown as a human-readable local date and time (e.g. "30 Sep 2026, 17:59"), never the raw `OffsetDateTime` debug/RFC string. The `Expired` state renders the same summary (behaviour unchanged). Re-check remains enabled whenever it is today.
- Q: What is "minimum window size" and how wide is the sign-out dialog? → A **[derived]**: minimum inner window size is **960×640** logical px (`main.rs` `with_min_inner_size`). **[default]**: the dialog width is `clamp(viewport_width − 2×space::XXL, 420, 560)` px; the intro and every consequence bullet wrap (never elide/truncate) within that width; the whole dialog (title, intro, every bullet, buttons) lies inside the viewport at 960×640 with 40 % text expansion, without internal scrolling for the currently registered consequence categories.
- Q: Default action in the sign-out dialog? → A **[derived]** (existing code + breakdown): Cancel receives initial keyboard focus once on open; Enter/Space activate the focused button, so pressing Enter immediately cancels; Escape and backdrop click behave as Cancel; "Sign out" keeps the Destructive variant. Button order unchanged (Cancel, then Sign out).
- Q: Which locales? → A **[derived]** (constitution X): every new string ships in **en-US and pt-BR** Fluent files; the existing Fluent key-parity test must pass.
- Q: Does reset violate "presentation only"? → A **[default]**: No — value domains, defaults, persistence format and account/sign-out logic are unchanged; reset is a UI affordance that writes the existing default through the existing setters.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read a category as a set of decisions (Priority: P1)

A user opens the Audio settings category. Related fields sit together in cards, each with a small group header. Each field's help text is indented under its label, and every numeric control shows its unit inside the control (for example "-1.0 dBFS", "50%"), so the five audio settings no longer read as ten identical lines.

**Why this priority**: Field legibility is the core value; it touches every category.

**Independent Test**: Open Audio; verify grouped cards with headers, indented help, and a unit on every slider value.

**Acceptance Scenarios**:

1. **Given** the Audio category, **When** shown, **Then** its fields appear in two cards headed "Output" (output device, buffer preset, Test output device) and "Level protection" (limiter ceiling, safe-volume switch, safe-volume cap).
2. **Given** any slider or numeric field, **When** shown, **Then** the value carries its unit inside the control (ceiling "dBFS", cap "%", nudge step "ms") and a caption beneath states its min–max range with unit.
3. **Given** a field with help text, **When** shown, **Then** the help starts ≥ 8 px right of its label's left edge and uses the secondary text style and weak text role.

---

### User Story 2 - Reset a changed field (Priority: P1)

A user who has changed a field from its default sees a reset control for that field only. Activating it restores the default and the control disappears.

**Why this priority**: Makes experimenting safe; explicit acceptance criterion.

**Independent Test**: Change the volume, confirm a per-field reset appears, use it, confirm default restored and reset gone.

**Acceptance Scenarios**:

1. **Given** a field at its default, **When** shown, **Then** no reset control is offered.
2. **Given** a field changed from its default, **When** shown, **Then** a reset control for that field is available and its accessible name includes the field name.
3. **Given** the reset control, **When** activated, **Then** only that field returns to its default (persisted through the same path as a manual change), the reset control disappears, and focus moves to the field's control.
4. **Given** the output device, locale, action buttons, Controls, Developer or plugin fields, **When** shown, **Then** no per-field reset is offered.

---

### User Story 3 - Jump to a matched field from search (Priority: P2)

A user searches settings and sees "category › field" results as today. Choosing a result opens the category and highlights the matched field so it is easy to find.

**Why this priority**: Improves discoverability; builds on existing search.

**Independent Test**: Search for a field in a lower category; choose it; the category opens and the field is visibly highlighted.

**Acceptance Scenarios**:

1. **Given** search results, **When** the user picks one, **Then** its category opens and the matched field is highlighted and scrolled into view.
2. **Given** a highlighted field, **When** 3 seconds pass or the user presses any key or pointer button, **Then** the highlight clears.
3. **Given** a highlight, **When** shown, **Then** it includes a ≥ 2 px outline around the whole field so it does not rely on colour alone.

---

### User Story 4 - Honest placeholder categories (Priority: P2)

Offline, and Privacy and diagnostics, state plainly that the area is not available yet (their real settings belong to later features). In the category row, such entries are visibly marked so they do not look identical to complete categories.

**Why this priority**: Removes misleading empty screens.

**Independent Test**: Open Offline; it shows settings or a clear "not available yet" statement, and its category-row entry carries a matching marker.

**Acceptance Scenarios**:

1. **Given** the Offline (or Privacy and diagnostics) category, **When** opened, **Then** it shows the category name as a header and one plain "not available yet" sentence — never the old "no settings yet in this update" line.
2. **Given** the category row, **When** shown, **Then** Offline and Privacy and diagnostics carry a "Coming soon" text badge (inline, pinned, and in the "More" menu) and complete categories do not.
3. **Given** a marked category, **When** a user reads it with assistive technology, **Then** its accessible name is "{category}, coming soon".

---

### User Story 5 - Account summary first (Priority: P2)

The Account category leads with a summary of signed-in identity, subscription tier, and when it was last verified, presented as one summary block rather than three equal lines. Re-check and sign-out actions sit beneath.

**Why this priority**: Most important account state is currently least legible.

**Independent Test**: Open Account signed in; identity, tier and last-verified appear as a summary with actions below.

**Acceptance Scenarios**:

1. **Given** a signed-in user, **When** Account is opened, **Then** identity, tier and last-verified time appear first as a summary, with re-check and sign-out beneath.
2. **Given** missing data, **When** shown, **Then** empty name shows "Name unavailable", `Unknown` tier shows "Not verified yet", and no validation shows "Never verified online" — never a blank or raw value.
3. **Given** a last-verified time, **When** shown, **Then** it is a human-readable local date and time, not a raw timestamp string.

---

### User Story 6 - Sign-out confirmation fully visible (Priority: P2)

The sign-out confirmation is wide enough to show its full list of what will be deleted at the minimum window size. Cancel stays the default action; sign-out stays styled as destructive.

**Why this priority**: Users must see consequences before committing.

**Independent Test**: At minimum window width, trigger sign-out; the full list is visible without truncation or scrolling off-screen; focus is on Cancel.

**Acceptance Scenarios**:

1. **Given** the 960×640 minimum window, **When** sign-out is requested, **Then** the entire consequence list is visible before the user commits.
2. **Given** the dialog, **When** opened, **Then** Cancel holds initial focus, Enter/Space activates Cancel, and Escape/backdrop click behave as Cancel.
3. **Given** the dialog, **When** shown, **Then** the sign-out action is styled destructive.

---

### Edge Cases

- Long translations (up to 40% text expansion) in group headers, labels, help, units and the dialog MUST NOT truncate or overlap.
- A field whose default equals its current value after manual edit back MUST hide its reset control.
- Not-yet-available categories have no search descriptors, so search never lands on them; opening them from the row or "More" menu shows their statement.
- Search target field collapsed or off-screen: it is scrolled into view before highlighting.
- High-contrast appearance: group cards, highlight, markers and destructive styling stay distinguishable without relying on colour alone.
- Account data partially unavailable (offline, unverified, `Expired` state): summary shows the defined fallbacks; re-check remains available as today.
- Empty search query: no results, so no highlight. Selecting a new result while a highlight is active moves the highlight to the new field (3 s timer restarts).
- A setting changed to a non-default value outside the UI (hand-edited settings file) shows its reset control on the next render.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: In Audio, Playback, Appearance, Language and Account, fields MUST be shown inside cards, each card with a small group header, using the groups defined in Clarifications (Audio: "Output", "Level protection"; Playback: "Connect device", "Markers"; Appearance: "Theme"; Language: "Language"; Account: summary card). Controls, Plugins, Developer and About bodies are unchanged.
- **FR-002**: Field help text MUST start ≥ `space::SM` (8 px) right of its label's left edge and use the secondary text style and weak/secondary text role, keeping the 72-character measure.
- **FR-003**: Every numeric control MUST display its unit inside the control's value box: limiter ceiling "dBFS", safe-volume cap "%", nudge step "ms"; the buffer preset keeps its "(~N ms)" suffix.
- **FR-004**: Each numeric field MUST show a localised range caption beneath the control (ceiling "-6.0 to -0.1 dBFS", cap "0 to 100%", nudge "1 to 1000 ms"), derived from the same bounds the control clamps to.
- **FR-005**: These fields MUST offer a per-field reset exactly when their current value differs from the default — buffer preset (Balanced), limiter ceiling (-1.0 dBFS), safe-volume switch (on), safe-volume cap (50%), theme (System), high contrast (off), device name (unset), nudge step (10 ms) — and MUST NOT offer it when equal. Output device, locale, action buttons, Controls, Developer and plugin fields MUST NOT offer this reset.
- **FR-006**: The per-field reset MUST restore only that field through the same setter/persistence path as a manual change, without confirmation; it MUST have visible text "Reset" and accessible name "Reset {field} to default", follow its control in keyboard order, and move focus to the field's control after activation.
- **FR-007**: Settings search MUST keep "category › field" results; selecting a descriptor result MUST open its category, scroll the field into view, focus its control and highlight the whole field (label, help, control). Plugin-field hits and Controls/Developer targets keep current behaviour.
- **FR-008**: The highlight MUST include a ≥ 2 px outline in a theme role (not colour-only), and MUST clear after 3 seconds or on the first subsequent key or pointer press, whichever is first.
- **FR-009**: Offline and Privacy and diagnostics MUST show the category name as a header and a plain, per-category localised "not available yet" sentence, replacing `placeholder-settings-category`; they remain selectable.
- **FR-010**: The category row MUST show a "Coming soon" text badge on Offline and Privacy and diagnostics (inline, pinned and "More" menu entries), included in the row's width measurement, with accessible name "{category}, coming soon"; complete categories MUST carry no marker.
- **FR-011**: Account MUST lead with one summary card — identity as the prominent line, tier and last-verified as secondary label–value lines — followed by Re-check and then the destructive Sign out; the `Expired` state uses the same layout.
- **FR-012**: Account summary MUST show "Name unavailable" for an empty identity, "Not verified yet" for `Tier::Unknown`, "Never verified online" when never validated, and render last-verified as a human-readable local date and time.
- **FR-013**: The sign-out confirmation MUST be `clamp(viewport_width − 64, 420, 560)` px wide, wrap (never truncate/elide) its intro and every consequence item, and lie fully inside the viewport at 960×640 with 40 % text expansion.
- **FR-014**: The sign-out confirmation MUST give Cancel initial focus (Enter/Space cancel), treat Escape/backdrop click as Cancel, and keep Sign out in the Destructive variant.
- **FR-015**: All new user-visible strings (group headers, units, range captions, "Reset" labels, not-available statements, "Coming soon", account fallbacks) MUST exist in en-US and pt-BR Fluent files, and layouts MUST tolerate 40 % text expansion.
- **FR-016**: New elements MUST use theme roles/tokens (no hard-coded colours), remain distinguishable in high-contrast, have accessible names, and be keyboard-operable in visual order.
- **FR-017**: Setting value domains, defaults, persistence format and account/sign-out behaviour MUST be unchanged; per-field reset only writes existing defaults through existing setters.

### Key Entities

- **Settings field**: a labelled control with help text, optional unit, optional range, current value and default value.
- **Field group**: a titled set of related fields shown in one card.
- **Category status**: whether a category is complete or not yet available, shown in the category row and its content.
- **Account summary**: signed-in identity, subscription tier, last-verified time.
- **Sign-out consequence list**: items deleted from the device on sign-out.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In Audio, 100% of fields appear under a group header, and 100% of numeric values (ceiling, cap; nudge step in Playback) show a unit and a range caption.
- **SC-002**: For the eight resettable fields (FR-005), 100% of changed-from-default fields offer a reset and 0% at default do.
- **SC-003**: Opening Offline or Privacy and diagnostics never shows an unexplained empty screen; the category row marks 100% of not-yet-available categories.
- **SC-004**: At the minimum window width, 100% of sign-out consequence items are visible before the user commits, and initial focus is on Cancel.
- **SC-005**: Choosing a descriptor search result lands on the category with the matched field focused, in view and highlighted in a single action; the highlight is gone after 3 s.
- **SC-006**: At 960×640, the Account summary (identity, tier, last verified) is the first content block and lies fully inside the visible scroll viewport without scrolling.
- **SC-007**: With 40% text expansion, no label, unit, header, marker or dialog text is truncated or overlaps.

## Assumptions

- The settings screen, category row and card/row/button components from earlier features exist and are reused.
- Minimum window size is the currently shipped minimum, 960×640 logical px (`crates/modplayer/src/main.rs`); the review's 800 px observation predates it.
- For Offline and Privacy and diagnostics, real settings belong to later features (003-offline-and-library/001, 007-operations-and-polish/001) and none exist in code today; this feature therefore states "not available yet" and marks them. Those later features remove the marker when they add settings.
- The shortcut reference (Controls category) is out of scope (011/005).
- Subscription and verification data come from the existing account state; no new data is collected.
- The highlight duration is 3 seconds, cleared earlier by any key or pointer press.
