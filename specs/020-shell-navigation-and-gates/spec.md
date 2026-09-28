# Feature Specification: Shell Navigation and Launch Gates

**Feature Branch**: `feature/020-shell-navigation-and-gates`

**Created**: 2026-09-25

**Status**: Draft

**Input**: User description: "Make the app's navigation honest: never show a control that does nothing, and never let a set of categories reflow into an unreadable pile. During the launch gates — the disclosure, sign-in and the audio device check — the five-item navigation rail is currently drawn and clickable, but clicking any item does nothing because the gate re-renders. A first-time user therefore meets five dead controls before meeting a live one. Hide the rail until the user reaches the main application, and give the gates the full window with a simple step indicator so the user knows how many steps remain. Inside the main application, give the rail a clear selected state that reads as navigation rather than a button, and keep each section's scroll position when the user returns to it. The settings categories — eleven of them — currently wrap into a second row at the app's smaller sizes, with the wrap point moving as translations lengthen. Keep them as a single horizontal row that never wraps: categories that do not fit collapse into an overflow control that lists the remainder, the selected category is always visible in the row, and the row remains keyboard-navigable end to end. Library tabs gain the count badges the tab component supports, so a user can see there are no saved albums before selecting that tab."

## Clarifications

### Session 2026-09-25 (clarify reviewer)

Sources: **[C]** `.specify/memory/constitution.md` v1.1.1 · **[B]** breakdown `009-window-and-shell/003-shell-navigation-and-gates.md` · **[X]** existing code/specs (`crates/modplayer-account/src/launch_flow.rs::next_step`, `crates/modplayer-ui/src/{app.rs,shell.rs,settings/mod.rs,library_view.rs,widgets/controls.rs}`, `crates/modplayer/src/main.rs`, `specs/016-list-row-and-panel-components/spec.md`). **[D]** = assumed default (no source settles it; conventional choice recorded).

1. **When exactly is the rail shown?** — *(X + B)* The rail is drawn if and only if `launch_step() == LaunchStep::Main` **and** no Device Check screen occupies the content area. `LaunchStep` is recomputed every frame (`next_step`), so a mid-session sign-out / session revocation that returns the app to the Sign-in step hides the rail again. "Kept visible for the remainder of the session" (old FR-005) is realigned to "visible whenever the app is in Main". → FR-001, FR-005.
2. **Settings-triggered Device Check preview ("Test output device")** — *(D, derived from the prompt's "never show a control that does nothing")* Today `show_main` draws the preview instead of the section while the rail stays clickable, so a rail click changes `shell.section` with no visible effect. The rail is hidden while that preview is open (it returns when the preview closes); no step indicator is shown for the preview (it is not a launch gate). → FR-001a.
3. **Keyboard section shortcuts during gates** — *(X)* `actions::dispatch` already runs only while `launch_step() == Main` with no Device Check open, so `Nav*` actions are inert exactly when the rail is hidden. This MUST remain true (regression test). → FR-001b.
4. **What the step indicator counts** — *(D)* The sequence is fixed at **three steps** in `next_step` order: 1 Welcome (disclosure), 2 Sign in, 3 Audio output check. The total is always 3 — the tier (and therefore whether step 3 applies) is unknown until sign-in finishes, so a path-dependent total would change mid-flow. Steps already satisfied on this launch (e.g. disclosure acknowledged on a previous run) render as complete; a non-Premium account goes from step 2 straight to Main. The indicator shows each step's label, marks complete / current / upcoming, and exposes text "Step *n* of 3: *label*" as its accessible name. Sub-views inside a step (Welcome's Decline / Privacy Notice; Sign-in's waiting-for-browser, checking, failed, store-unavailable, tier-result sub-states) are the same step and never advance the number. Strings are externalized in en-US and pt-BR *(C X, NFR-7.1)*. → FR-002, FR-003.
5. **Non-Premium tier explanation** — *(X)* The Free / Unknown tier-result content is a sub-state of the Sign-in step (`sign_in.rs::show_tier_result`); it shows step 2 of 3 and no rail. → Edge Cases.
6. **"Full window" for gates** — *(X)* The `Panel::left("shell-nav-rail")` is not added at all during gates (not drawn empty or zero-width), so the `CentralPanel` spans the full window width. Notifications keep their existing bottom-right placement during gates (unchanged, 019). → FR-004.
7. **Rail selected state** — *(D, mirrors 016's tab rule "a stroke, never a filled accent background")* The selected rail item has **no filled button background**; it is marked by an `accent`-coloured indicator bar on the item's leading (left) edge, full item height, plus `text_primary` label colour; unselected items use `text_secondary` with hover fill only. Accessibility: selected item sets `selected = true`; accessible name stays the exact un-uppercased `tr(key)` (unchanged, FR-019 of 014) *(C X, NFR-6.2)*. Contrast of the indicator against the rail surface ≥ 3:1 in both themes and in high-contrast mode (017). → FR-006.
8. **What "keep scroll position" covers** — *(D)* Per section, the app restores the view that was showing when the user left (Library: selected tab and any open detail page; Search: query and results; Settings: selected category; Now Playing; Plugins) **and** that view's vertical scroll offset. Restoration is exact to within 1 logical pixel, clamped to the new maximum if content shrank meanwhile. Retention is in-memory for the running session only; it resets on sign-out / session revocation (library content is cleared then, 002 design note 7) and on restart. Switching Library *tabs* is not a section round trip and keeps today's behaviour. → FR-007.
9. **Which settings categories stay in the row when not all fit** — *(D)* Categories keep canonical order (`SettingsCategory::ALL`). The row shows the longest prefix of categories that fits alongside the overflow control; if the selected category is not in that prefix, the last prefix item is moved into overflow and the selected category is shown in the row **at the trailing end of the visible categories, immediately before the overflow control**, so visual order stays: visible items (canonical order), selected-if-pinned, overflow. The overflow list shows the remaining categories in canonical order. Visible labels are never truncated or ellipsized. → FR-009, FR-010.
10. **Overflow control shape** — *(D)* A trailing "More" button (label externalized, accessible name "More settings categories", `expanded` state exposed) opening a vertical menu of the overflowed categories. Selecting an item selects that category (which is then pinned into the row per item 9) and closes the menu. The control is omitted entirely when all eleven fit. → FR-009.
11. **Keyboard model for the row** — *(D, egui/convention)* Tab / Shift+Tab traverse visible categories left→right then the overflow control; Enter/Space activates; on the open menu Up/Down move, Enter selects, Escape closes and returns focus to the overflow control. Focus ring per 015. *(C X, NFR-6.1)* → FR-011.
12. **Resize while the overflow menu is open** — *(D)* The menu closes whenever the visible/overflow partition changes; keyboard focus moves to the overflow control if still present, else to the selected category. (Replaces the earlier "contents recompute while open" edge case.) → Edge Cases.
13. **Minimum width and the 40 % test** — *(X)* Minimum window inner size is 960 × 640 logical px (`main.rs` `with_min_inner_size`, 018). The row's available width is the window width minus the rail. The 40 % test lengthens **every** category label to ⌈1.4 × its character count⌉ (padding with a wide glyph) and asserts, at 960 px width, that every drawn row item shares one line (identical top y, row height = one item height). Also run with the pt-BR locale. → FR-008, FR-012, SC-002.
14. **Library tab counts already exist** — *(X)* 016-list-row-and-panel-components FR-014/FR-015 already renders each tab's count (mono, `0` for empty, **no count while `library_status().loading`**, count adjacent to — not inside — the tab's accessible name), and `library_view.rs` implements it. This feature does not re-implement it; User Story 4 is a verification/regression story. FR-013/FR-014 are realigned to 016's semantics. No count badges are added to the rail. → FR-013, FR-014.
15. **Traceability** — *(C Governance)* Requirements cite NFR-6.1 (keyboard-operable), NFR-6.2 (accessible names), NFR-7.1 (externalized strings, en-US + pt-BR) where they apply.

No item met the materiality test for escalation: the one real product fork (settings sidebar vs single row) was already decided in the breakdown [B], and everything else has a derivation or a conventional default that is cheap to change later.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - No dead controls during first launch (Priority: P1)

A first-time user with no account configured launches the app. Before they reach the main application they pass through the disclosure screen, sign-in, and the audio device check. At every one of those steps they see only the content of that step and a simple indicator of how many steps remain — never a set of navigation controls that look clickable but do nothing.

**Why this priority**: This is the app's first impression. Five dead controls before one live one teaches a new user that the app's navigation cannot be trusted, at the exact moment it most needs their confidence.

**Independent Test**: Start the app with no account configured (fresh install) and step through disclosure → sign-in → device check. At every step, confirm no navigation controls are drawn or reachable by keyboard, and a step indicator communicating progress is visible.

**Acceptance Scenarios**:

1. **Given** the app starts with no account configured, **When** the welcome/disclosure screen is shown, **Then** no navigation rail or its items are visible or keyboard-reachable, and a step indicator reads "Step 1 of 3: Welcome".
2. **Given** the user has acknowledged the disclosure and reached sign-in, **When** the sign-in step is shown, **Then** no navigation rail is visible, and the step indicator reads "Step 2 of 3: Sign in" with Welcome marked complete.
3. **Given** a Premium account has signed in and the audio device check is shown, **When** the device check step is shown, **Then** no navigation rail is visible, and the step indicator reads "Step 3 of 3" with steps 1–2 marked complete.
4. **Given** the device check completes, **When** the app reaches the main application, **Then** the navigation rail becomes visible for the first time, with its five sections.
5. **Given** a gate step is re-shown after a retry (e.g. sign-in fails and the user retries, or the device check answers "No, try another"), **When** the same step re-renders, **Then** the navigation rail remains hidden and the step indicator still shows the same step number out of 3.
6. **Given** the user is in the main application, **When** they sign out (or the session is revoked) and the app returns to the Sign-in step, **Then** the rail is hidden again and the step indicator reads "Step 2 of 3".
7. **Given** a gate step is shown, **When** the user presses any section keyboard shortcut, **Then** nothing happens (no section change is recorded for later).

---

### User Story 2 - Settings categories never wrap (Priority: P1)

A user opens Settings at the app's smallest supported window size, or with the app's UI language set to one whose labels run longer than English. The eleven settings categories stay on a single row; whichever ones do not fit are reachable from a clearly-marked overflow control, and the category currently selected is always visible directly in the row.

**Why this priority**: An unreadable, size-dependent pile of wrapped category chips is a P1 finding in the source review and breaks at the app's own stated minimum window size — not an edge case, the default smallest state.

**Independent Test**: Resize the app to its minimum supported window width, open Settings, and confirm the category row occupies exactly one line with an overflow control holding whatever does not fit; repeat with category labels lengthened by 40% and confirm the row still occupies one line.

**Acceptance Scenarios**:

1. **Given** the settings screen is shown at the app's minimum window width, **When** the category row renders, **Then** it occupies exactly one line, and any categories that do not fit are reachable from an overflow control rather than wrapping to a second line.
2. **Given** a category name is lengthened by 40% (simulating a longer-language translation), **When** the category row renders at the minimum window width, **Then** the row still occupies exactly one line.
3. **Given** the currently selected category would not otherwise fit in the visible row, **When** the row renders, **Then** the selected category is shown directly in the row, immediately before the overflow control, and is absent from the overflow list.
4. **Given** the settings screen is shown, **When** a user navigates using only the keyboard, **Then** Tab reaches every visible category left→right and then the overflow control; Enter opens the overflow menu, Up/Down reach every overflowed category in canonical order, Enter selects one, and Escape closes the menu returning focus to the overflow control.
5. **Given** the window is widened so all eleven categories now fit, **When** the row re-renders, **Then** no overflow control is shown and every category appears directly in the row.

---

### User Story 3 - Rail reads as navigation and remembers where you were (Priority: P2)

A user in the main application looks at the navigation rail and can immediately tell which section they are in, because the selected item is visually distinct as a navigation state rather than a pressed button. When they leave a section they had scrolled through — for example the library, scrolled halfway down — to use another section (such as search) and come back, they find themselves exactly where they left off instead of scrolled back to the top.

**Why this priority**: Once the rail is honestly visible (User Story 1) and the settings row is legible (User Story 2), this story is about the everyday quality of using the rail — important, but it does not block a first launch or break at minimum window size the way the first two do.

**Independent Test**: In the main application, select each rail section in turn and confirm the selected item's visual treatment is distinct from an unselected, clickable item. Separately, scroll the library section halfway down, switch to search, then return to library and confirm the scroll position is unchanged.

**Acceptance Scenarios**:

1. **Given** the user is in the main application, **When** they select a rail section, **Then** that section's rail item shows an accent leading-edge indicator bar and primary label colour with no filled background, while unselected items show neither, and the selected item exposes `selected = true` to assistive technology.
2. **Given** the user has scrolled the library section halfway down, **When** they switch to search and then return to library, **Then** the library section is scrolled to the same position it was left at.
3. **Given** the user has scrolled a section that supports scrolling, **When** they switch to a different rail section and back to the original one, **Then** the returned-to section shows the same sub-view (library tab / open detail page, search query, settings category) at the same scroll offset (±1 logical px), repeatable across at least two round trips.
4. **Given** the user signs out and signs back in, **When** the main application is reached, **Then** every section starts at its default view scrolled to the top.

---

### User Story 4 - Library tab counts are visible before selecting (Priority: P3)

A user viewing the library sees a count next to each library tab (for example, Albums, Playlists), so they can tell at a glance — before clicking — whether a tab such as saved albums has anything in it.

**Why this priority**: A useful, low-risk polish item that removes one small class of surprise (clicking into an empty tab) but does not affect first launch, minimum-size layout, or core navigation correctness. The behaviour was already delivered by 016-list-row-and-panel-components (FR-014/FR-015; Clarifications 14); this story is verification and regression coverage only — no new badge behaviour.

**Independent Test**: With a library that has zero saved albums, open the library screen and confirm the Albums tab shows a count of zero without needing to select the tab first.

**Acceptance Scenarios**:

1. **Given** the library has finished loading and has zero items of a given kind (e.g. no saved albums), **When** the library screen is shown, **Then** the corresponding tab displays a visible `0` (mono figures) without the user selecting that tab.
2. **Given** the library's item counts change (e.g. an album is saved), **When** the library screen is next drawn, **Then** the tab's displayed count reflects the current count.
3. **Given** the library is still loading, **When** the tab row renders, **Then** no tab shows a count.

---

### Edge Cases

- What happens when a launch gate step is retried or fails (e.g. sign-in cancelled, device check "No, try another")? The rail stays hidden and the step indicator continues to reflect the user's true current step — it never counts a retry as an additional step, and never reveals the rail early.
- What happens when the non-Premium (Free/Unknown tier) explanation is shown? It is a sub-state of the Sign-in step: rail hidden, indicator "Step 2 of 3". Continuing from it goes to Main (no step 3).
- What happens when the window is resized while the settings overflow menu is open? The menu closes whenever the visible/overflow partition changes; focus moves to the overflow control if still present, else to the selected category. The selected category stays in the row (never only in the overflow list).
- What happens when a settings search result selects a category that is currently in the overflow list? That category becomes selected and is pinned into the row per FR-010.
- What happens when every one of the eleven categories independently grows by 40%, not just one? The row still occupies exactly one line, with the overflow control absorbing whatever does not fit.
- What happens if the user restarts the app, or signs out, after leaving a section scrolled partway? Positions are not restored: retention is in-memory for the running signed-in session only.
- What happens when the section's content shrank while the user was away (e.g. library refreshed with fewer items)? The restored offset is clamped to the new maximum scroll offset.
- What happens when the user opens "Test output device" from Settings? The rail is hidden while that Device Check preview is shown and returns when it closes; no step indicator is shown.
- What happens when a rail section that has no scrollable content (e.g. a section that always fits without scrolling) is left and returned to? There is no observable change, since there was no scroll position to preserve.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The navigation rail (`Panel::left("shell-nav-rail")`) MUST be drawn if and only if `launch_step() == LaunchStep::Main` and no Device Check screen is shown. While hidden, neither the rail nor any of its items may be rendered or exposed to keyboard or assistive-technology navigation. This covers Welcome (incl. Decline / Privacy Notice sub-views), every Sign-in sub-state (incl. Free/Unknown tier result), and the launch Device Check. *(Clarifications 1, 5)*
- **FR-001a**: While the Settings-triggered Device Check preview ("Test output device") is shown, the rail MUST be hidden; it MUST reappear when the preview closes. No step indicator is shown for the preview. *(Clarifications 2)*
- **FR-001b**: Section navigation actions (`NavLibrary`, `NavSearch`, `NavNowPlaying`, `NavPlugins`, `NavSettings`) MUST have no effect — immediate or deferred — whenever the rail is hidden. *(Clarifications 3)*
- **FR-002**: Every launch gate step MUST show a step indicator for a fixed three-step sequence — 1 Welcome, 2 Sign in, 3 Audio output check — marking each step complete / current / upcoming, labelling each step, and exposing the accessible name "Step *n* of 3: *label*". Steps already satisfied (e.g. disclosure acknowledged on an earlier launch) render complete. All indicator strings MUST be externalized with en-US and pt-BR translations. *(Clarifications 4; NFR-6.2, NFR-7.1)*
- **FR-003**: The step number MUST be derived solely from the current `LaunchStep` (Welcome = 1, SignIn = 2, DeviceCheck = 3); retries, failures, sub-views and re-renders within a step MUST NOT change it. *(Clarifications 4)*
- **FR-004**: While the rail is hidden, the rail panel MUST NOT be added to the frame at all (not drawn empty or at zero width), so the gate content spans the full window width. *(Clarifications 6)*
- **FR-005**: Whenever the rail is shown (FR-001), it MUST contain all five sections (Library, Search, Now Playing, Plugins, Settings) in their existing order. If the app leaves Main (sign-out, session revocation), the rail is hidden again per FR-001. *(Clarifications 1)*
- **FR-006**: The selected rail item MUST have no filled background and MUST be marked by an `accent`-coloured leading-edge indicator bar spanning the item's height plus `text_primary` label colour; unselected items MUST have no indicator bar, use `text_secondary`, and may show only the hover fill. The indicator MUST meet ≥ 3:1 contrast against the rail surface in light, dark and high-contrast appearances. The selected item MUST expose `selected = true`; each item's accessible name MUST remain the exact un-uppercased `tr(key)`. *(Clarifications 7; NFR-6.2)*
- **FR-007**: For each rail section, the system MUST retain in memory — for the running signed-in session — the sub-view shown when the user left it (Library tab and open detail page; Search query and results; Settings category) and that view's vertical scroll offset, and restore both on return to within ±1 logical px, clamping to the current maximum offset if content shrank. Retained state MUST be reset on sign-out / session revocation and is not persisted across restarts. *(Clarifications 8)*
- **FR-008**: The settings category row MUST lay out all its items (visible categories and overflow control) on a single horizontal line at every window width ≥ the minimum inner width of 960 logical px; it MUST NOT wrap. *(Clarifications 13)*
- **FR-009**: When not all eleven categories fit, the row MUST show the longest canonical-order (`SettingsCategory::ALL`) prefix that fits together with a trailing "More" overflow control (accessible name "More settings categories", exposing expanded/collapsed state, strings externalized) whose menu lists the remaining categories in canonical order. Visible labels MUST NOT be truncated, ellipsized or clipped. When all eleven fit, the overflow control MUST NOT be drawn. Selecting a category from the menu MUST select it and close the menu. *(Clarifications 9, 10; NFR-6.2, NFR-7.1)*
- **FR-010**: The selected category MUST always be drawn in the row and MUST NOT appear in the overflow list. If it is outside the fitting prefix, the prefix's last item(s) move to overflow as needed and the selected category is drawn immediately before the overflow control. *(Clarifications 9)*
- **FR-011**: The row MUST be fully keyboard-operable: Tab / Shift+Tab traverse visible categories in visual order then the overflow control; Enter/Space activates; in the open menu Up/Down move between items, Enter selects, Escape closes and returns focus to the overflow control. When the visible/overflow partition changes (e.g. resize) while the menu is open, the menu MUST close and focus MUST move to the overflow control if still present, else to the selected category. *(Clarifications 11, 12; NFR-6.1)*
- **FR-012**: FR-008–FR-010 MUST hold at 960 px window width when every category label is lengthened to ⌈1.4 × its character count⌉, and with the pt-BR locale. *(Clarifications 13)*
- **FR-013**: Library tab counts MUST keep 016-list-row-and-panel-components FR-014/FR-015 behaviour: once `library_status().loading` is false, every tab shows its list's length in `mono` figures beside its label, including `0`, without the tab being selected; no count is shown while loading; the count is not part of the tab's accessible name. This feature adds regression coverage only. *(Clarifications 14)*
- **FR-014**: The displayed library tab counts MUST equal the current in-memory list lengths on every frame the library screen is drawn. *(Clarifications 14)*

### Key Entities

- **Launch Gate Step**: One of the three fixed steps before the main application — 1 Welcome (disclosure), 2 Sign in (including tier-result sub-states), 3 Audio output check — mapped 1:1 from `LaunchStep`; has a position (1–3) and a complete / current / upcoming state, used to drive the step indicator and to determine whether the navigation rail is shown.
- **Navigation Rail**: The main application's persistent set of section entry points (Library, Search, Now Playing, Plugins, Settings); has a visibility state (hidden during gates, visible in the main application) and a currently selected section.
- **Rail Section**: One destination reachable from the navigation rail; holds its last sub-view and vertical scroll offset in memory while the user is elsewhere, reset on sign-out.
- **Settings Category**: One of the eleven settings categories; has a label, a position in the row's visual order, and a state of either directly visible in the row or held inside the overflow control.
- **Overflow Control**: The control that appears in the settings category row when not all categories fit; holds the categories that do not fit and exposes them for keyboard and pointer selection.
- **Library Tab**: One of the library's tabs (e.g. Albums, Playlists); has a label and an item count shown as a badge.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of first launches with no account configured show zero navigation controls until the main application is reached.
- **SC-002**: At the app's minimum supported window width, the settings category row occupies exactly one line in 100% of renders, including when every category label is lengthened by 40%.
- **SC-003**: Every settings category, including those inside the overflow control, is reachable and selectable using only the keyboard, in the same visual order as the row.
- **SC-004**: A user who scrolls a rail section partway, switches to a different section, and returns finds the scroll position unchanged in 100% of same-session round trips.
- **SC-005**: At every gate step the step indicator shows "Step *n* of 3" with *n* equal to the `LaunchStep` position, verified for each step and after a retry in each step.
- **SC-006**: A user can tell whether a library tab (e.g. saved albums) has any items before selecting it, in 100% of library views.

## Assumptions

- The navigation rail's five sections (Library, Search, Now Playing, Plugins, Settings) and the launch gate sequence (disclosure → sign-in → device check → main, with a non-Premium browse-only explanation reached instead of device check for non-Premium accounts) are as established by the 001-walking-skeleton and 002-first-launch-and-sign-in features; this feature changes their visibility and indicator behavior, not their order or count.
- "Full window" for gate steps means the content area reclaims the horizontal space previously reserved for the rail; it does not change the gate screens' own content or copy, which is out of scope (covered separately per the source breakdown).
- Scroll position retention is a same-session, in-memory convenience: it does not survive restart or sign-out (FR-007).
- The overflow control is a trailing "More" button opening a vertical menu (FR-009, FR-011).
- Library tab counts already exist (016-list-row-and-panel-components FR-014/FR-015, implemented in `library_view.rs`); this feature only verifies and regression-tests them (FR-013).
- The minimum supported window size is 960 × 640 logical px (`main.rs` `with_min_inner_size`, 018-window-sizing-and-responsive-dock).
- The settings sidebar alternative (source § 7 open question 3) is out of scope; the breakdown chose the single non-wrapping row.
