# Feature Specification: Design Tokens, Type Scale, and Spacing

**Feature Branch**: `feature/014-design-tokens-and-type-scale`

**Created**: 2026-09-22

**Status**: Clarified — specify and clarify passes complete; no open [NEEDS CLARIFICATION] markers and no items escalated for human decision; ready for `/speckit-plan`

**Input**: User description: "Give ModPlayer a single source of visual truth, so the app stops looking like an unstyled prototype and no screen can invent its own colour or size again. Define a named set of design tokens covering typography, spacing, radius and colour, applied once per frame to the whole application rather than per widget. Typography is six roles on the platform's default family: a display role for the now-playing track title and the welcome heading, a title role for screen headings, a small uppercase section role for panel and group headers, a body role for row titles and field labels, a muted secondary role for artist/album lines, help text and metadata, and a tabular monospace role for every number a user compares — timestamps such as `0:35.204`, decibel readings, CPU and memory figures, and durations. Spacing is a 4-pixel scale (4, 8, 12, 16, 24, 32) and radius has three steps; panels are separated by space rather than hairline rules, and body copy is capped at roughly 72 characters so paragraphs stop running the full window width. Colour becomes semantic — primary, secondary and disabled text, a base and a raised surface, an accent, and positive, warning and danger roles — with a light and a dark value for every role. Secondary text must reach at least a 4.5:1 contrast ratio against its surface in both themes; today the light theme measures 2.96:1 on the artist, album, duration and owner lines, which fails the product's stated accessibility minimum. Every colour and font-size literal currently scattered through the views — plugin health dots, the over-ceiling meter red, opaque white marker labels, ad-hoc 9-point monospace — moves into the token layer, restoring the project's existing rule that no view names a colour. Acceptance: when the app runs in the light theme and a secondary text sample is measured against its background, the contrast ratio is at least 4.5:1. When a list row is displayed, its title and its secondary line render at visibly different size and weight. When the source tree is searched for colour literals outside the theme module, nothing is found. When a marker's timestamp and a plugin's CPU figure are shown in adjacent rows, their digits align in a fixed-width column."

**Source**: [ModPlayer-UI-UX-Review.md § 4.1 No design tokens](../autonomous/ModPlayer-UI-UX-Review.md#41-no-design-tokens), [§ 4.2 Measured contrast](../autonomous/ModPlayer-UI-UX-Review.md#42-measured-contrast-wcag-2x-sampled-from-the-captures), [§ 4.4 Typography and text](../autonomous/ModPlayer-UI-UX-Review.md#44-typography-and-text), [§ 5.1 Type scale](../autonomous/ModPlayer-UI-UX-Review.md#51-type-scale-single-family-platform-default-textstyle-overrides), [§ 5.2 Spacing and radius](../autonomous/ModPlayer-UI-UX-Review.md#52-spacing-and-radius), [§ 5.3 Semantic colour](../autonomous/ModPlayer-UI-UX-Review.md#53-semantic-colour-both-themes-contrast-verified), [§ 3.9 Cross-theme](../autonomous/ModPlayer-UI-UX-Review.md#39-cross-theme); [ModPlayer-Software-Specification.md](../autonomous/ModPlayer-Software-Specification.md) NFR-6.4 (colour never the sole carrier of meaning), NFR-6.5 (contrast minimums in both themes), NFR-7.4 (40% text-expansion headroom, informs the 72-character measure); [011-plugin-ui-contributions/contracts/ui-panels.md](../011-plugin-ui-contributions/contracts/ui-panels.md) contract A4 ("every colour/font comes from `ui.visuals()`/`ui.style()`; the panel module contains no colour literals"), which this feature restores host-wide. **Prerequisites**: assumes the full MVP UI through [013-key-and-tempo-plugin](../013-key-and-tempo-plugin/spec.md); every design-foundation wave from this feature onward builds on the tokens defined here.

**Scope boundary**: This feature defines the token set (typography, spacing, radius, colour) and applies it host-wide in place of today's scattered literals and toolkit defaults. It does **not** restructure any layout, change any component's interaction behaviour, add hover/focus/pressed states, redesign list rows, tabs, or panel cards (those are [002-control-variants](../autonomous/breakdown/008-design-foundation/002-control-variants.md) and [003-list-row-and-panel-components](../autonomous/breakdown/008-design-foundation/003-list-row-and-panel-components.md)), or add the high-contrast appearance variant ([004-high-contrast-appearance](../autonomous/breakdown/008-design-foundation/004-high-contrast-appearance.md)).

## Clarifications

### Session 2026-09-22 (specify pass)

Resolution ladder applied: (D) derived from an authoritative source (the source review document, the constitution, or an already-ratified prior feature spec), (A) assumed conventional default. No item below met the materiality bar for human escalation.

- Q: The source document's own open question — is a custom font family in scope, or does the type scale stay on each platform's default family? → A (D, source § 7 open question 27/the slice's own text: "The slice assumes the platform default"): all six type-scale roles use the platform default font family; no custom typeface is bundled or selected by this feature. A future feature may introduce one without breaking the role names, since a role is defined by size/weight/case, not by family.
- Q: `MARKER_PALETTE` is explicitly theme-independent by design (a marker keeps its colour across a theme switch, § 5.3) — does that exempt it from any contrast requirement? → A (D, § 5.3's own text: "gains a documented minimum 3:1 against both `surface.base` values"): marker colours are not tokenized as semantic roles and do not change value between light and dark, but each one MUST be verified to hold at least 3:1 contrast against both `surface.base` values; a marker hue that cannot clear 3:1 in one theme is adjusted (not exempted), since the requirement is on the fixed palette's members, not on a per-theme value.
- Q: `text.disabled` is listed in § 5.3 at a 3:1 minimum rather than the 4.5:1 that applies to `text.primary`/`text.secondary` — is that a lower bar being applied to text, contradicting NFR-6.5? → A (D, § 5.3's own annotation "3:1 (non-text)"): disabled controls are treated as non-text UI components under WCAG's own distinction between text contrast (4.5:1) and non-text/inactive-component contrast (3:1), consistent with NFR-6.5's "recognized guideline minimums" rather than a blanket 4.5:1 applied everywhere; only `text.primary` and `text.secondary` carry the 4.5:1 text floor.

### Session 2026-09-22 (clarify pass)

Same ladder: (D) derived from an authoritative source, (A) assumed
conventional default. Every contrast figure quoted below is a WCAG 2.x
relative-luminance ratio computed from the § 5.3 token values themselves;
where a computed figure contradicts the source table, the computation is
stated and the conflict is resolved in favour of the source's *stated floor*,
because a floor is a requirement and a swatch is a starting point. No item
reached the materiality bar for human escalation.

**Token identity and placement**

- Q: Which module is "the one token module", and does the existing `theme.rs`
  survive? → A (D, § 5 preamble: "All of this belongs in
  `crates/modplayer-ui/src/theme.rs`, applied once per frame through
  `egui::Style`/`Visuals`"): `crates/modplayer-ui/src/theme.rs` (expanded, a
  `theme/` directory module if it outgrows one file) **is** the token module.
  Its existing residents — `MARKER_PALETTE`, `marker_color`, `overlay_color`,
  `paint_host_glyph` — stay where they are; they are already the sanctioned
  exception to `contracts/ui-panels.md` A4 and remain so.
- Q: Are the type-scale numbers points, physical pixels, or logical pixels? →
  A (A, conventional for this toolkit): logical pixels at a 1.0 scale factor.
  The platform's own DPI factor is applied by the toolkit on top; the token
  values are never pre-multiplied by it.
- Q: NFR-6.6 requires text to scale to 200%, and the source's open question 4
  asks whether a text-scale setting should come first. Does this feature add
  one? → A (A, and scope boundary): no. This feature adds no text-scale
  setting and no reflow work (that is `UX-40`, a separate later feature).
  The six role sizes are defined as *base* values that a future global scale
  factor multiplies, so introducing that setting later changes one multiplier
  rather than six literals.

**Typography**

- Q: FR-003 says all six roles use "the platform's default font family", but
  `mono` must be fixed-width — is that a contradiction? → A (A, and the
  prompt's own words "a tabular monospace role"): the five proportional roles
  (`display`, `title`, `section`, `body`, `secondary`) use the platform
  default **proportional** family; `mono` uses the platform default
  **monospace** family. § 5.1's "single family" means a single proportional
  family — no second display or branding typeface — not that numbers give up
  fixed-width advance. No typeface is bundled either way.
- Q: What makes `mono` "tabular" testably? → A (A): every digit glyph in the
  `mono` role has the same advance width, which a monospace face satisfies by
  construction. Testable without a screenshot: two `mono` strings of equal
  character count lay out to the same width.
- Q: § 5.1 asks for semibold, but the toolkit's default font stack may expose
  only one weight per family. What happens then? → A (A, conventional
  graceful degradation): a role's weight resolves to the platform default
  family's semibold face, or to its bold face if no semibold face exists. If
  the running platform exposes neither, the role falls back to the single
  available weight and its distinction is carried by size (and, for `section`,
  by uppercase) — US2's "visibly different size and/or weight" is written with
  "and/or" precisely so a one-weight platform still passes.
- Q: § 5.1 gives `section` `+0.04 em` letter-spacing, which the toolkit may
  not express. Blocker? → A (A): letter-spacing is applied where the toolkit
  supports per-glyph advance adjustment and omitted where it does not. Its
  absence is not a failure of any acceptance scenario; `section` remains
  distinguishable by size, weight, and uppercase alone.
- Q: Who uppercases `section` headers — the string catalogue or the renderer?
  → A (D, Principle X "All host strings are externalized; English and pt-BR
  ship first"): the renderer, at draw time, using the active locale's
  uppercasing rules. Catalogue entries stay in their natural case in every
  locale; no locale gains a duplicate all-caps entry, and no uppercase form is
  baked into a translated string.
- Q: Does defining six named roles leave every widget the views do not
  explicitly restyle sitting on toolkit defaults, making FR-002's
  "applied once per frame" hollow? → A (A): no — the toolkit's five built-in
  text styles are **all** remapped onto roles as part of the once-per-frame
  application: `Heading` → `title`, `Body` → `body`, `Button` → `body`,
  `Small` → `secondary`, `Monospace` → `mono`. `display` and `section` are
  added as two further named styles. Any widget that names no style therefore
  still renders from the scale.

**Colour values**

- Q: The spec states contrast floors but never the actual colour values, so
  `plan` would have to invent them. What are they? → A (D, § 5.3's table):
  the nine roles' light/dark values are now written into the spec body as
  FR-010's table. They are normative starting values, subject to the next two
  items.
- Q: § 5.3's `surface.raised` values do not satisfy § 5.3's own "≥1.2:1 vs
  base" annotation — light `#f5f5f7` on `#ffffff` computes to **1.09:1** and
  dark `#1e1e22` on `#141417` to **1.11:1**. Which wins? → A (D, floor over
  swatch, plus § 3.9's complaint that light "gives almost no surface
  separation" — the very defect the floor exists to fix): the 1.2:1 floor
  governs and both values are adjusted to clear it, keeping the source's
  direction of elevation (raised is *darker* than base in light, *lighter*
  than base in dark — the grey-card-on-white-page pattern). Light
  `surface.raised` darkens to approximately `#eaeaec` or darker; dark
  `surface.raised` lightens to approximately `#25252a` or lighter. The exact
  values are whatever the automated contrast test accepts.
- Q: `accent` is used as a *fill* behind text (selection, a filled primary
  button), but the nine roles name no colour for text drawn on it. What
  colour is that text? → A (A, forced and conventional — a fill role needs an
  "on" pair): a tenth role, `text.on-accent`, is added: light `#ffffff`
  (5.8:1 on `#0a63c9`), dark `#141417` (7.5:1 on `#5aa9ff`). It MUST hold
  ≥4.5:1 against `accent` in its own theme. Without it a view would have to
  name white itself, re-creating exactly the defect FR-018 forbids.
- Q: The toolkit's visuals carry far more slots than nine roles (widget fills,
  strokes, faint and extreme backgrounds, hyperlink, selection). What fills
  the rest? → A (A): every remaining slot is **derived from the ten roles**,
  never left at a toolkit default that names its own colour — `surface.base`
  drives the window, panel, and text-entry backgrounds; `surface.raised`
  drives card, faint, and widget backgrounds; `text.primary` drives default
  widget text; `text.secondary` drives weak text; `text.disabled` drives
  inactive widget text; `accent` drives selection fill and hyperlink, paired
  with `text.on-accent`. Chrome that frames content (nav rail, plugin dock,
  transport bar) uses `surface.raised`; scrolling content areas use
  `surface.base`.
- Q: FR-008's "1 px line at 8% foreground opacity" is a colour — is it a
  literal, or an eleventh role? → A (D, § 5.2 says "8 % foreground"): neither.
  It is `text.primary` at 8% alpha, computed inside the token module and
  exposed from there as a `divider` value. The nine (now ten) semantic roles
  are unchanged, and no call site writes an alpha or a colour.
- Q: Do the text roles' contrast floors apply only against `surface.base`, or
  against `surface.raised` too — since most text sits inside cards? → A (A,
  conventional): against **both** surfaces in its own theme. All § 5.3 values
  already clear this (e.g. `text.secondary` `#5b5b60` computes 6.75:1 on
  `#ffffff` and 6.2:1 on `#f5f5f7`), so it costs nothing and closes the hole
  where a role passes on the page and fails on the card.
- Q: § 5.3's `text.disabled` `#8e8e93` computes to 3.25:1 on white, which
  *lowers* today's measured 8.84:1 — but FR-012 as drafted says this feature
  "does not require lowering it". Which is it? → A (D, § 5.3's table governs,
  and § 4.2 itself calls 8.84:1 "arguably too strong for a disabled state"):
  the table wins. Disabled text does get lighter in the light theme, landing
  at 3.25:1 light / 3.52:1 dark — both above the 3:1 non-text floor. FR-012 is
  realigned below to stop saying otherwise.

**Marker palette — four entries fail the new floor**

- Q: § 5.3 says `MARKER_PALETTE` "gains a documented minimum 3:1 against both
  `surface.base` values", and `theme.rs` already claims the palette was
  "Chosen for >= 3:1". Is documenting it enough? → A (D + computation): no —
  the claim does not survive the new surfaces, and neither source foresaw
  this. Holding 3:1 against **both** `#ffffff` and `#141417` confines a
  colour's relative luminance to the band `0.121 ≤ L ≤ 0.300`. Measured
  against the new pair, today's eight entries are:

  | # | Hue | Value | vs `#ffffff` | vs `#141417` | |
  |---|---|---|---|---|---|
  | 0 | red | `#D94F4F` | 4.05:1 | 4.54:1 | pass |
  | 1 | blue | `#3E8EDE` | 3.43:1 | 5.37:1 | pass |
  | 2 | green | `#4CAF50` | **2.78:1** | 6.58:1 | **fail** |
  | 3 | amber | `#E09B1A` | **2.37:1** | 7.77:1 | **fail** |
  | 4 | violet | `#9C5CE0` | 4.18:1 | 4.40:1 | pass |
  | 5 | teal | `#00ACB0` | **2.79:1** | 6.58:1 | **fail** |
  | 6 | pink | `#DE6FB4` | **2.98:1** | 6.16:1 | **fail** |
  | 7 | olive | `#8F9A4B` | 3.05:1 | 6.03:1 | pass |

  The four failures are **re-toned** — hue and mutual distinguishability
  preserved, luminance lowered into the band — rather than exempted, because
  § 5.3 states the rule and only the fact (that today's palette breaks it) is
  new. The palette stays theme-independent and stays keyed by `PaletteIndex`,
  so persisted markers keep their identity; only the rendered hue shifts.
- Q: Two of those failures are the entries `overlay_color` maps plugin
  `OverlayColor::Positive`/`Warning` onto. Should plugin overlays be repointed
  at the new `positive`/`warning` semantic roles instead? → A (D, scope
  boundary + the ratified 011 contract): **no.** `contracts/overlays-settings-
  notify.md` § 1.2 (O8) fixes that mapping so a plugin's "good"/"caution"
  reads consistently with a host marker's; repointing it is a cross-feature
  contract change, and this feature's scope boundary forbids behaviour
  changes. The two entries' *values* shift with the re-toning above; the
  *mapping* does not. § 5.3's "`positive`/`warning`/`danger` replace the
  literals" names `plugins_view.rs` and `peak_meter.rs` only, and is read as
  written.
- Q: Do plugin-contributed panels need an API change to reach the new roles? →
  A (D, `contracts/ui-panels.md` A4 + FR-002): no. Tokens are applied to the
  shared style/visuals once per frame, and plugin panels are already bound by
  A4 to read `ui.visuals()`/`ui.style()`, so they inherit the scale with no
  new plugin API surface — which also means Principle IX's written
  change-request requirement is not triggered by this feature.

**Verification mechanics**

- Q: SC-001/SC-002 say contrast "is measured" — by the screenshot sampler, or
  by a test? → A (D, Principle VIII "Test what the NFRs promise" + the
  Governance manual sign-off): **both, at different times.** An automated unit
  test computes the WCAG 2.x ratio directly from the token values for every
  (text role × theme × surface) pair and fails below that role's floor; it
  also asserts `surface.raised` ≥1.2:1 vs base, `text.on-accent` ≥4.5:1 vs
  accent, and all eight `MARKER_PALETTE` entries ≥3:1 against both bases. The
  screenshot sampler (`target/manual-walk/contrast.py`) stays as the
  quickstart manual scenario's evidence that the rendered pixels match the
  token values.
- Q: SC-004/SC-005 say "a source-tree search returns zero results" — a search
  run by whom, over what? → A (A, conventional — an unautomated search
  regresses on the next PR that is not reviewed by hand): an automated
  repository test, running alongside the existing `fmt`/`clippy`/`test`/`deny`
  gates, scans `crates/modplayer-ui/src/**` and `crates/modplayer/src/**` for
  colour constructors, hex colour literals, and font-size literals, and fails
  on any hit. Exclusions, and only these: the token module itself
  (`theme.rs`/`theme/`), and test code (`#[cfg(test)]` blocks and `tests/`
  directories). Today's baseline for that scan is 27 occurrences across 9
  files, of which the token module's 10 are sanctioned and the other 17 — in
  `artwork.rs`, `markers.rs`, `plugins_view.rs`, `plugin_assets.rs`,
  `plugin_overlays.rs`, `widgets/peak_meter.rs`, `widgets/initials.rs`,
  `waveform/paint.rs` — are what this feature removes.

**Spacing, radius, and measure**

- Q: `full` radius has no number. What is it? → A (A, conventional): a radius
  of at least half the element's height — a pill/stadium shape — not a fixed
  pixel value, so a badge stays fully rounded at any height.
- Q: FR-006 caps "body copy" at ~72 characters — which strings count? → A (A):
  multi-line prose blocks only — help text, field descriptions, empty-state
  copy, settings explanations, the privacy notice, and the getting-started
  text. It does **not** apply to single-line labels, row titles, table cells,
  tooltips, or notification messages (notification truncation is § 5.4's and
  003's scope). The measure is 72 × the advance width of the `body` role's
  `0` glyph, applied as a maximum width, never as a fixed or minimum width.
- Q: How does an existing arbitrary gap become a scale step? → A (A): the
  toolkit's 8/4 px defaults already coincide with `sm`/`xs` and map straight
  across; any other existing value rounds to the nearest defined step, with a
  tie rounding **up** (toward more breathing room, which is the feature's
  stated intent).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Secondary text is readable in the light theme (Priority: P1)

A user reading a track's artist, album, duration, or a plugin's owner line in the light theme can read it comfortably against its background, instead of the low-contrast grey the app ships today.

**Why this priority**: This is the feature's lead acceptance line and its only measured accessibility failure (§ 4.2: light-theme secondary text measures 2.96:1, below the product's own 4.5:1 minimum, NFR-6.5). It is the single highest-impact, most concretely testable outcome in the slice.

**Independent Test**: With the app in the light theme, sample a secondary-text element (e.g. a library row's artist · album · duration line) against its background using the project's contrast tool and confirm the ratio is at least 4.5:1.

**Acceptance Scenarios**:

1. **Given** the app is running in the light theme, **When** a secondary text sample (artist, album, duration, or owner line) is measured against its background, **Then** the contrast ratio is at least 4.5:1.
2. **Given** the app is running in the dark theme, **When** the same kind of secondary text sample is measured, **Then** the contrast ratio remains at least 4.5:1 (no regression from today's passing 5.12:1).

---

### User Story 2 - Row titles and their secondary lines are visually distinct (Priority: P1)

A user scanning a list can tell a row's title apart from its secondary details at a glance, because the two use noticeably different size and weight, not the single uniform text style the app uses today.

**Why this priority**: This is the feature's second acceptance line and the most visible day-to-day consequence of a type scale — every list, search result, and plugin table benefits immediately (§ 4.4: "one size, one weight, one family throughout").

**Independent Test**: Display any list row that carries a title and a secondary line (library row, search result, plugin entry) and confirm the two render at visibly different size and/or weight.

**Acceptance Scenarios**:

1. **Given** a list row with a title and a secondary line is displayed, **When** the row is rendered, **Then** the title uses the `body` type role and the secondary line uses the `secondary` type role, and the two are visibly different in size and/or weight.
2. **Given** a screen heading (e.g. a screen title) and a panel or group header (e.g. "Markers", "Effect chain") are both visible, **When** the screen is rendered, **Then** the screen heading uses the `title` role and the panel/group header uses the small uppercase `section` role, and the two are visibly distinct from each other and from body text.
3. **Given** the Now Playing track title and the first-launch welcome heading, **When** either is rendered, **Then** it uses the `display` role, the largest and most prominent role in the scale.

---

### User Story 3 - Numbers a user compares line up in a fixed-width column (Priority: P2)

A user comparing numeric values across adjacent rows — a marker's timestamp against another marker's, or one plugin's CPU reading against another's — sees the digits align in a column instead of drifting left and right with proportional spacing.

**Why this priority**: This is the feature's fourth acceptance line. It is narrower in reach than US1/US2 (it affects only numeric fields) but is explicitly called out as its own measurable outcome, and today's proportional numerals (§ 4.4) make every table of numbers harder to scan.

**Independent Test**: Display two rows whose numeric fields are of the same kind (e.g. two markers' timestamps, or two plugins' CPU percentages) and confirm their digits occupy the same horizontal positions.

**Acceptance Scenarios**:

1. **Given** a marker's timestamp (e.g. `0:35.204`) and a plugin's CPU figure are shown in adjacent rows, **When** both are rendered, **Then** both use the `mono` type role with tabular (fixed-width) figures, and their digit columns align.
2. **Given** any decibel reading, CPU/memory figure, or duration is displayed anywhere in the app, **When** it is rendered, **Then** it uses the `mono` role rather than the `body` or `secondary` role.

---

### User Story 4 - Panels breathe and paragraphs stay a comfortable width (Priority: P3)

A user viewing the app sees panels separated by visible space instead of thin dividing lines, and reads paragraphs of body copy that stop at a comfortable line length instead of stretching the full width of a maximized window.

**Why this priority**: This is a readability and polish outcome named in the prompt and § 5.1/§ 5.2, but it is less load-bearing than the contrast fix (US1) or the type hierarchy (US2) — it improves comfort without which the app remains usable, if visually cramped.

**Independent Test**: Resize the window wide, display a screen with a paragraph of help or metadata text, and confirm the paragraph wraps at roughly 72 characters rather than the window's full width; separately, confirm adjacent panels are separated by spacing rather than a hairline rule.

**Acceptance Scenarios**:

1. **Given** a window wide enough that a paragraph of body copy would otherwise span its full width, **When** the paragraph is rendered, **Then** it wraps at approximately 72 characters per line.
2. **Given** two panels appear adjacent to one another, **When** they are rendered, **Then** they are separated by the `xl` (32px) spacing token rather than a `ui.separator()` hairline rule.
3. **Given** any two elements that use the spacing scale, **When** their gap is measured, **Then** it is one of the six defined steps (4, 8, 12, 16, 24, 32) and never an arbitrary pixel value.

---

### User Story 5 - No screen invents its own colour or size again (Priority: P2)

Anyone reading the app's source code — today or in a future change — finds that every colour and font size comes from the one token module; no view file declares a colour literal or an ad-hoc font size of its own.

**Why this priority**: This is the feature's third acceptance line and its structural goal: without it, the contrast fix in US1 is one screen's fix, not the app's guarantee, and the next screen added is free to regress. It restores an existing, previously-violated project rule (`contracts/ui-panels.md` A4).

**Independent Test**: Search the source tree for colour literals (hex codes, `Color32::` constructors, named toolkit colours) and hard-coded font-size values outside the theme/token module and confirm none are found outside that module.

**Acceptance Scenarios**:

1. **Given** the full source tree, **When** it is searched for colour literals outside the theme module, **Then** nothing is found — including today's known offenders: the plugin health dots, the over-ceiling meter red, the opaque white marker/overlay labels, and any other view-level colour constant.
2. **Given** the full source tree, **When** it is searched for ad-hoc font-size literals outside the theme module, **Then** nothing is found — including today's known offenders: the 9-point monospace marker label, the initials avatar text, and the waveform paint label.
3. **Given** the application switches from light to dark theme, **When** the switch occurs, **Then** every screen repaints with the new theme's values with zero code changes in any individual view — because every view reads tokens rather than holding its own values.

---

### Edge Cases

- A marker colour in the existing, theme-independent `MARKER_PALETTE` fails to reach 3:1 contrast against one of the two `surface.base` values: the colour value is adjusted (the palette's identity/hue intent is preserved as closely as possible) rather than being left non-compliant or made theme-dependent. Four of the eight are in this case today (FR-015); a re-toned entry's `PaletteIndex` is unchanged, so an existing marker keeps its slot and simply renders in the corrected hue.
- A re-toned palette entry is one the plugin overlay contract maps onto (entries 2 and 3): the plugin-facing mapping stays as ratified and the plugin's overlay simply resolves to the corrected value (FR-015a).
- The platform's default font family exposes no semibold face: the role falls back to bold, or to the single available weight, and the role's distinction rests on size and (for `section`) uppercase — US2 passes on "size and/or weight" (FR-003).
- A paragraph is displayed inside a container narrower than the ~72-character measure (e.g. a docked side panel): the 72-character figure is a maximum, not a fixed width, so the text wraps at the container's own width, narrower than 72 characters, without truncation.
- A UI element is both disabled and would otherwise carry text (e.g. a disabled button's label): it uses `text.disabled` at its 3:1 non-text-component minimum, not the 4.5:1 text minimum that applies to active `text.primary`/`text.secondary` content (Clarifications).
- A spacing gap smaller than the scale's minimum (`xs`, 4px) exists today from the toolkit's own defaults (egui's default 8/4px spacing): it is normalized to the nearest defined step rather than left as an arbitrary sub-scale value.
- A value that is semantically `positive`/`warning`/`danger` (e.g. a peak meter reading right at the over-ceiling threshold) sits exactly on a boundary between two colour roles: the existing threshold logic already in the view (unchanged by this feature, § 4.1) selects the role; this feature only supplies the token values the selection resolves to, it does not change the thresholds themselves.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST define one token module — `crates/modplayer-ui/src/theme.rs`, expanded in place (a `theme/` directory module if it outgrows one file) — that is the single source of typography, spacing, radius, and colour values for the entire application; no other module MUST declare a colour, font size, spacing, or radius value of its own. *(Prompt; § 4.1; § 5 preamble; `contracts/ui-panels.md` A4; Clarifications)*
- **FR-002**: Tokens MUST be applied once per frame to the whole application's rendering style/visuals, not read or set per widget or per view — so that a theme switch or a token change is visible everywhere on the next frame with zero per-view code changes. *(Prompt: "applied once per frame to the whole application rather than per widget"; § 5 preamble)*
- **FR-003**: The system MUST define exactly six typography roles, sized in logical pixels at a 1.0 scale factor: `display` (22 / semibold — Now Playing track title, welcome heading), `title` (17 / semibold — screen headings, detail headers), `section` (13 / semibold, +0.04em letter-spacing, uppercase — panel and group headers), `body` (14 / regular — row titles, field labels, paragraphs), `secondary` (13 / regular, muted — artist/album lines, help text, metadata), and `mono` (13 / tabular figures — timestamps, dB, CPU/memory, durations). The five proportional roles use the platform's default proportional font family and `mono` uses the platform's default monospace family; no typeface is bundled. A role's weight resolves to that family's semibold face, or its bold face where no semibold face exists, or — where the platform exposes neither — to the single available weight, in which case the role's distinction is carried by size and, for `section`, by uppercase. Letter-spacing is applied where the toolkit supports per-glyph advance adjustment and omitted where it does not; its omission is not a failure of any acceptance scenario. *(Prompt; § 5.1 table; Clarifications)*
- **FR-003a**: The toolkit's built-in text styles MUST all be remapped onto the roles as part of the once-per-frame application — `Heading` → `title`, `Body` → `body`, `Button` → `body`, `Small` → `secondary`, `Monospace` → `mono` — with `display` and `section` added as two further named styles, so that a widget naming no style still renders from the scale rather than from a toolkit default. *(FR-002; Clarifications)*
- **FR-003b**: The `section` role's uppercasing MUST be applied at draw time using the active locale's uppercasing rules; externalized string catalogue entries MUST remain in their natural case in every locale, with no all-caps duplicate entry and no uppercase form baked into a translated string. *(Principle X "All host strings are externalized"; NFR-7.1; Clarifications)*
- **FR-004**: The `mono` role MUST use tabular (fixed-width) numerals — every digit glyph carrying the same advance width — so that any two numeric values of the same kind shown in adjacent rows (e.g. two markers' timestamps, or two plugins' CPU readings) align their digit columns. Testable without a screenshot: two `mono` strings of equal character count MUST lay out to the same width. *(Prompt acceptance line 4; § 5.1; Clarifications)*
- **FR-005**: Every value in the app that is a timestamp (e.g. `0:35.204`), a decibel reading, a CPU or memory figure, or a duration MUST render using the `mono` role, replacing today's proportional rendering of these fields. *(Prompt; § 4.4)*
- **FR-006**: Body copy MUST be capped at a maximum measure of 72 × the advance width of the `body` role's `0` glyph, wrapping within that measure rather than spanning the full window width; a container narrower than the measure wraps at its own width (Edge Cases). The measure is a maximum, never a fixed or minimum width. It applies to multi-line prose blocks only — help text, field descriptions, empty-state copy, settings explanations, the privacy notice, and the getting-started text — and MUST NOT be applied to single-line labels, row titles, table cells, tooltips, or notification messages. *(Prompt; § 5.1: "Body copy gets a max measure of ~72 characters (UX-08)"; Clarifications)*
- **FR-007**: The system MUST define a 4-pixel spacing scale with exactly six steps: `xs` 4, `sm` 8, `md` 12, `lg` 16, `xl` 24, `xxl` 32. Every padding, margin, and gap value in the application MUST be one of these six steps. An existing arbitrary value is normalized to the nearest defined step, with a tie rounding up. *(Prompt; § 5.2; Clarifications)*
- **FR-008**: Panels MUST be visually separated from one another by spacing (the `xl` step) rather than by a hairline rule (`ui.separator()`); panel content MUST use `lg` padding. Rows MUST use `sm` vertical padding and, where a divider is still used, a 1px line at 8% foreground opacity. That divider colour MUST be computed inside the token module as `text.primary` at 8% alpha and exposed from there as a `divider` value — it is neither a semantic role of its own nor an alpha written at a call site. *(Prompt: "panels are separated by space rather than hairline rules"; § 5.2; Clarifications)*
- **FR-009**: The system MUST define a radius scale with exactly three steps: `sm` 4 (inputs, chips), `md` 8 (cards, panels), and `full` (badges), where `full` is a radius of at least half the element's height — a pill/stadium shape at any height — rather than a fixed pixel value. *(Prompt; § 5.2; Clarifications)*
- **FR-010**: The system MUST define the semantic colour roles below, each carrying one light-theme value and one dark-theme value. These values are normative; where one conflicts with its own stated floor, the floor governs and the value is adjusted (FR-013). *(Prompt; § 5.3 table; Clarifications)*

  | Role | Light | Dark | Floor |
  |---|---|---|---|
  | `text.primary` | `#1c1c1e` | `#f2f2f7` | 4.5:1 on both surfaces |
  | `text.secondary` | `#5b5b60` | `#a8a8b0` | 4.5:1 on both surfaces |
  | `text.disabled` | `#8e8e93` | `#6c6c72` | 3:1 (non-text) |
  | `surface.base` | `#ffffff` | `#141417` | — |
  | `surface.raised` | adjusted per FR-013 (from `#f5f5f7`) | adjusted per FR-013 (from `#1e1e22`) | ≥1.2:1 vs base |
  | `accent` | `#0a63c9` | `#5aa9ff` | 4.5:1 on `surface.base` |
  | `text.on-accent` | `#ffffff` | `#141417` | 4.5:1 on `accent` |
  | `positive` | `#1f7a44` | `#4caf50` | 4.5:1 on `surface.base` |
  | `warning` | `#8a5a00` | `#e0a92a` | 4.5:1 on `surface.base` |
  | `danger` | `#b3261e` | `#ff6b5e` | 4.5:1 on `surface.base` |

- **FR-010a**: `text.on-accent` MUST exist as a role because `accent` is used as a fill behind text (selection, a filled primary button); without it a view would have to name a colour itself, re-creating the defect FR-018 forbids. It MUST hold at least 4.5:1 against `accent` in its own theme (the listed values measure 5.8:1 light, 7.5:1 dark). *(Clarifications)*
- **FR-010b**: Every colour slot the rendering toolkit exposes that the roles do not name directly MUST be derived from the roles, never left at a toolkit default that supplies its own colour: `surface.base` drives window, panel, and text-entry backgrounds; `surface.raised` drives card, faint, and widget backgrounds; `text.primary` drives default widget text; `text.secondary` drives weak text; `text.disabled` drives inactive widget text; `accent` drives selection fill and hyperlink, paired with `text.on-accent` for text drawn on it. Chrome that frames content (nav rail, plugin dock, transport bar) uses `surface.raised`; scrolling content areas use `surface.base`. *(FR-002; Clarifications)*
- **FR-011**: `text.secondary` MUST reach at least a 4.5:1 contrast ratio against **both** `surface.base` and `surface.raised` in both the light and dark themes, replacing today's light-theme value that measures 2.96:1 against the product's stated accessibility minimum. `text.primary` MUST likewise meet or exceed 4.5:1 against both surfaces in both themes (today it already does against its single surface, at 7.66:1 light / 12.41:1 dark). *(Prompt acceptance line 1; § 4.2 measured table; NFR-6.5; Clarifications)*
- **FR-012**: `text.disabled` MUST meet at least a 3:1 contrast ratio against both surfaces in both themes, treated as a non-text UI component per WCAG's distinction rather than the 4.5:1 text floor (Clarifications). The § 5.3 values land at 3.25:1 light and 3.52:1 dark — this is a deliberate reduction from today's light-theme 8.84:1, which § 4.2 records as "arguably too strong for a disabled state". *(§ 5.3 table; § 4.2; Clarifications)*
- **FR-013**: `surface.raised` MUST hold at least 1.2:1 contrast against `surface.base` in both themes, so that a raised card or panel reads as separated from the page behind it — fixing today's near-invisible light-theme surface separation (§ 3.9: "light's near-white `#f9f9f9` panel background against white scroll areas gives almost no surface separation"). § 5.3's listed values do not satisfy this — light `#f5f5f7` on `#ffffff` measures 1.09:1 and dark `#1e1e22` on `#141417` measures 1.11:1 — so both MUST be adjusted until the floor is met, keeping the source's direction of elevation (raised darker than base in light, lighter than base in dark): light to approximately `#eaeaec` or darker, dark to approximately `#25252a` or lighter. *(§ 3.9; § 5.3 table; Clarifications)*
- **FR-014**: `accent`, `positive`, `warning`, and `danger` MUST each meet at least 4.5:1 contrast against `surface.base` in both themes (the § 5.3 values measure 5.8/7.5, 5.4/6.6, 5.9/8.7 and 6.5/6.6 respectively). *(§ 5.3 table)*
- **FR-015**: The existing theme-independent `MARKER_PALETTE` MUST NOT change value between the light and dark themes (a marker keeps its colour across a theme switch, by design) and MUST stay keyed by `PaletteIndex` so persisted markers keep their identity, but every colour in it MUST hold at least 3:1 contrast against both `surface.base` values — which confines each entry's relative luminance to the band 0.121 ≤ L ≤ 0.300. Four of today's eight entries fail against the new surfaces and MUST be re-toned (hue and mutual distinguishability preserved, luminance lowered into the band): index 2 green `#4CAF50` (2.78:1), index 3 amber `#E09B1A` (2.37:1), index 5 teal `#00ACB0` (2.79:1), and index 6 pink `#DE6FB4` (2.98:1). Indices 0, 1, 4, and 7 already pass and are left alone. `theme.rs`'s existing doc claim that the palette was "Chosen for >= 3:1" MUST be corrected to state the surfaces it is verified against. *(§ 5.3; Clarifications)*
- **FR-015a**: The ratified mapping in `contracts/overlays-settings-notify.md` § 1.2 (011, O8) from plugin `OverlayColor::Positive`/`Warning` to `MARKER_PALETTE[2]`/`[3]` MUST NOT be repointed at the new `positive`/`warning` roles by this feature — that is a cross-feature contract change, which the scope boundary forbids. The two entries' values shift with FR-015's re-toning; the mapping itself is unchanged. *(Scope boundary; `contracts/overlays-settings-notify.md` § 1.2; Clarifications)*
- **FR-015b**: This feature MUST NOT add or change any plugin API surface. Because tokens are applied to the shared style/visuals once per frame (FR-002) and plugin panels are already bound by `contracts/ui-panels.md` A4 to read `ui.visuals()`/`ui.style()`, plugin-contributed UI inherits the scale with no new API — so Principle IX's written change-request requirement is not triggered. *(Principle IX; `contracts/ui-panels.md` A4; Clarifications)*
- **FR-016**: Every colour literal currently scattered through the views MUST move into the token module and be replaced at its call site with a reference to the appropriate semantic role, specifically: the plugin health dots and the peak meter's over-ceiling red MUST use the `positive`/`warning`/`danger` roles (selected by the view's existing, unchanged threshold logic — Edge Cases); the opaque white marker and overlay labels MUST use a token role rather than a hard-coded white. *(Prompt; § 4.1: named offenders `plugins_view.rs:195-201`, `widgets/peak_meter.rs:38`, `markers.rs:256,273,282`, `plugin_overlays.rs:177`)*
- **FR-017**: Every ad-hoc font-size literal currently scattered through the views MUST move into the token module and be replaced at its call site with the appropriate type-scale role, specifically the 9-point monospace marker label, the initials-avatar text, and the waveform paint label. *(Prompt; § 4.1: named offenders `markers.rs:281`, `initials.rs:76`, `waveform/paint.rs:116`)*
- **FR-018**: After this feature ships, a search of the source tree for colour literals outside the token module MUST find none, and a search for font-size literals outside the token module MUST find none — restoring the project's existing rule that no view names a colour (`contracts/ui-panels.md` A4, extended host-wide by this feature). *(Prompt acceptance line 3)*
- **FR-018a**: That search MUST be an automated repository test running alongside the existing `fmt`/`clippy`/`test`/`deny` gates, not a manual grep: it scans `crates/modplayer-ui/src/**` and `crates/modplayer/src/**` for colour constructors, hex colour literals, and font-size literals and fails the build on any hit. The only permitted exclusions are the token module itself (`theme.rs`/`theme/`) and test code (`#[cfg(test)]` blocks and `tests/` directories). Baseline at the start of this feature: 27 occurrences across 9 files, of which the token module's 10 are sanctioned and the remaining 17 — in `artwork.rs`, `markers.rs`, `plugins_view.rs`, `plugin_assets.rs`, `plugin_overlays.rs`, `widgets/peak_meter.rs`, `widgets/initials.rs`, `waveform/paint.rs` — are what this feature removes. *(Principle VII quality gates; Principle VIII; Clarifications)*
- **FR-018b**: Contrast MUST be verified by an automated unit test that computes WCAG 2.x relative-luminance ratios directly from the token values — covering every (text role × theme × surface) pair against that role's floor, `surface.raised` ≥1.2:1 against `surface.base`, `text.on-accent` ≥4.5:1 against `accent`, `accent`/`positive`/`warning`/`danger` ≥4.5:1 against `surface.base`, and all eight `MARKER_PALETTE` entries ≥3:1 against both `surface.base` values — failing below any floor. The screenshot sampler (`target/manual-walk/contrast.py`) remains the quickstart manual scenario's evidence that rendered pixels match the token values, per the constitution's Manual Scenario Sign-Off. *(Principle VIII; Governance › Manual Scenario Sign-Off; § 4.2; Clarifications)*
- **FR-019**: This feature MUST NOT change any component's layout, interaction behaviour, hover/focus/pressed states, or the structure of list rows, tabs, or panel cards, and MUST NOT introduce a high-contrast appearance variant — those are the responsibility of the 002 (control variants), 003 (list row and panel components), and 004 (high-contrast appearance) features in this same design-foundation wave. *(Scope boundary)*
- **FR-020**: This feature MUST NOT add a text-scale setting or reflow behaviour (NFR-6.6 / `UX-40`, a separate later feature). The six role sizes MUST be defined as base values that a single future global scale factor multiplies, so that introducing that setting later changes one multiplier rather than six literals. *(Scope boundary; § 4.6; § 7 open question 4; Clarifications)*

### Key Entities

- **Design Token**: A named value — a size, a spacing amount, a radius, or a colour — referenced by its role name everywhere it is used, defined exactly once in the token module. Replaces every literal previously written directly into a view.
- **Type Scale Role**: One of six named text styles (`display`, `title`, `section`, `body`, `secondary`, `mono`), each carrying a size, a weight, and, where applicable, letter-spacing/case and tabular-figure behaviour.
- **Spacing Scale**: The six-step sequence (`xs` 4 … `xxl` 32) used for every padding, margin, and gap in the application.
- **Radius Scale**: The three-step sequence (`sm`, `md`, `full`) used for every rounded corner in the application.
- **Semantic Colour Role**: A named colour purpose (`text.primary`, `text.secondary`, `text.disabled`, `surface.base`, `surface.raised`, `accent`, `text.on-accent`, `positive`, `warning`, `danger`), each carrying one light-theme value and one dark-theme value, with a documented minimum contrast against its surface (FR-010's table).
- **Derived Value**: A colour the token module computes from a role rather than storing as its own role — today the `divider` line (`text.primary` at 8% alpha) and every toolkit visuals slot the roles do not name directly (FR-010b). Derived values live in the token module and are never written at a call site.
- **Marker Palette**: The existing, theme-independent set of eight colours assigned to user-created markers, keyed by `PaletteIndex` so persisted markers keep their identity across a re-tone. Four of the eight are re-toned by this feature to reach the 3:1 floor against both `surface.base` values (FR-015); their hues and mutual distinguishability are preserved, and the ratified plugin `OverlayColor` mapping onto entries 2 and 3 is unchanged (FR-015a).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In the light theme, a sample of secondary text (artist, album, duration, or owner line) measures at least 4.5:1 contrast against its background — on both `surface.base` and `surface.raised` — up from today's 2.96:1.
- **SC-002**: In the dark theme, the same kind of secondary text sample continues to measure at least 4.5:1 contrast against both surfaces.
- **SC-003**: 100% of list rows sampled across the app render their title and secondary line at visibly different size and/or weight.
- **SC-004**: The automated literal scan (FR-018a) over `crates/modplayer-ui/src/**` and `crates/modplayer/src/**` returns zero colour-literal hits outside the token module and test code, down from 17 today.
- **SC-005**: The same scan returns zero font-size-literal hits outside the token module and test code.
- **SC-009**: The automated contrast test (FR-018b) passes every floor in FR-010's table in both themes, including all eight `MARKER_PALETTE` entries at ≥3:1 against both `surface.base` values and `surface.raised` at ≥1.2:1 against `surface.base`.
- **SC-006**: When a marker's timestamp and a plugin's CPU or memory figure are displayed in adjacent rows, their digit columns align to the pixel.
- **SC-007**: Switching the app between light and dark theme repaints every screen correctly with no visible unstyled or stale-coloured element, and requires no code change in any individual view.
- **SC-008**: A paragraph of body copy displayed in a window wide enough to otherwise span it instead wraps at approximately 72 characters per line.

## Assumptions

- The type scale uses each platform's default proportional family for the five proportional roles and its default monospace family for `mono`; no custom typeface is bundled or selected by this feature (Clarifications).
- Token sizes and spacing are logical pixels at a 1.0 scale factor; the platform's DPI factor is applied by the toolkit on top and is never pre-multiplied into a token (Clarifications).
- `text.on-accent` is added as a tenth role because `accent` is used as a fill behind text; the source's nine-role table could not express that without a view naming a colour (Clarifications).
- Where a § 5.3 swatch and a § 5.3 floor disagree, the floor governs and the swatch is adjusted — this affects `surface.raised` in both themes (FR-013) and four `MARKER_PALETTE` entries (FR-015).
- The plugin `OverlayColor` → `MARKER_PALETTE` mapping ratified in 011 is out of scope to change; only the mapped entries' values move (Clarifications).
- The underlying UI toolkit already supports applying a style/visuals configuration once per frame to the whole application (established by the MVP UI through 013); this feature does not evaluate or change that mechanism, only the values it is given.
- Light and dark are the only two theme variants in scope; the high-contrast variant (008-design-foundation/004) is a separate, later feature that extends this token structure rather than replacing it.
- The existing threshold logic that selects between `positive`/`warning`/`danger` for a given value (e.g. a peak meter's over-ceiling detection) is unchanged by this feature; only the colour values those roles resolve to are introduced here.
- `MARKER_PALETTE`'s existing hue choices are preserved as closely as possible; only a colour that fails the new 3:1 floor against either surface is adjusted (Clarifications).
- No layout restructuring occurs in this slice: row grids, tab bars, panel card boundaries, and component interaction states are unchanged here and are the explicit scope of the 002 and 003 features in this wave.
