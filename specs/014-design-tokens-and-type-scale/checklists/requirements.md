# Specification Quality Checklist: Design Tokens, Type Scale, and Spacing

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-22
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs) beyond what this codebase's own spec convention already carries (established by 001–013: this is a technical product where specs name concrete APIs/toolkit calls to stay testable; consistent with `contracts/ui-panels.md` A4 and the source review document itself, which is why FR-008's `ui.separator()` reference and the Assumptions' toolkit note are kept)
- [x] Focused on user value and business needs (readable text, distinguishable hierarchy, aligned numbers, a maintainable single source of visual truth)
- [x] Written for the project's established technical-stakeholder audience, consistent with every prior feature spec in this repository
- [x] All mandatory sections completed (User Scenarios & Testing, Requirements, Success Criteria, Assumptions)

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous (exact sizes, weights, hex-adjacent contrast ratios, and named offending call sites)
- [x] Success criteria are measurable (contrast ratios, zero-result searches, pixel alignment, wrap width)
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined (2–3 per user story)
- [x] Edge cases are identified (marker palette contrast floor, narrow-container wrapping, disabled-text contrast tier, sub-scale spacing normalization, threshold-boundary colour selection)
- [x] Scope is clearly bounded (Scope boundary section; FR-019; explicit hand-off to 002/003/004)
- [x] Dependencies and assumptions identified (Assumptions section)

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria (cited to the source review document's sections and the master specification's NFR IDs)
- [x] User scenarios cover primary flows (contrast fix, type hierarchy, tabular numerals, spacing/measure, zero-literal enforcement)
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into the specification beyond the project's own established convention

## Notes

- All items pass on the first validation iteration; no spec updates were required after the initial draft.
- The one open question left by the source document (custom font family vs. platform default) was resolved in the Clarifications section using the source's own stated assumption — no human escalation was needed.

### Clarify pass, 2026-09-22

All items still pass, with the following substantive changes recorded in
Clarifications › Session 2026-09-22 (clarify pass) and realigned into the
requirement body. No item reached the materiality bar for human escalation,
so no `NEEDS HUMAN` section exists.

- **Two source-internal contradictions found by computing the ratios**, both
  resolved floor-over-swatch: `surface.raised` fails its own ≥1.2:1
  annotation in *both* themes (1.09:1 light, 1.11:1 dark) and is adjusted
  (FR-013); four of eight `MARKER_PALETTE` entries fail the 3:1 floor against
  the new pure-white `surface.base` (green 2.78, amber 2.37, teal 2.79, pink
  2.98) and are re-toned (FR-015). Neither source anticipated these, because
  both predate the surface values changing.
- **Concrete token values are now in the spec body** (FR-010's table) rather
  than only in the source review, so `plan`/`tasks` do not have to invent them.
- **A tenth role, `text.on-accent`, was added** (FR-010a) — the nine-role set
  could not colour text drawn on an `accent` fill without a view naming a
  colour, which FR-018 forbids.
- **Verification is now automated** (FR-018a, FR-018b): the "search the source
  tree" and "measure the contrast" acceptance lines become a repository scan
  and a unit test computing WCAG ratios from the token values, per Principle
  VIII; the screenshot sampler stays as manual sign-off evidence.
- **Cross-feature contracts explicitly held fixed**: the 011 plugin
  `OverlayColor` → `MARKER_PALETTE` mapping is not repointed (FR-015a) and no
  plugin API surface changes, so Principle IX is not triggered (FR-015b).
- **Toolkit-shaped gaps defaulted rather than escalated**: `mono` resolves to
  the platform monospace family, built-in text styles are all remapped onto
  roles (FR-003a), `section` uppercases at draw time per locale (FR-003b),
  missing semibold degrades to bold then to size-only distinction, and
  unsupported letter-spacing is not a failure.
