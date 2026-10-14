# Contract: Account Summary and Sign-out Confirmation

**Feature**: 028-settings-fields-and-account | Covers FR-011–FR-014, FR-016, FR-017, SC-004, SC-006, SC-007.

## Account summary (FR-011, FR-012, SC-006)

| ID | Rule |
|---|---|
| A1 | In `SessionState::Active` and `Expired`, the first content block of Settings › Account is one `panel_card` titled `account-summary-title`, containing in order: identity (body, strong), tier line, last-verified line. |
| A2 | Tier line = secondary-style label `account-tier` + value (`tier-premium` / `tier-free` / `account-tier-unverified` for `Tier::Unknown`). Last-verified line = secondary-style label `account-last-verified` + value. |
| A3 | Identity = `display_name`; if `display_name.trim()` is empty → `account-identity-unavailable` ("Name unavailable"). Never a blank line. |
| A4 | `last_validated_at == None` → `account-never-verified` ("Never verified online"). `Some(t)` → `format_last_verified(t, local_offset_at(t))`: en-US "30 Sep 2026, 17:59" (day, short month from `date-month-short-N`, year, 24 h `HH:MM`, local time). Never the `Display`/RFC 3339 form. |
| A5 | Below the card, in order: Re-check help + Re-check button (standard), divider/destructive gap, Sign out (`Variant::Destructive`). Re-check enabled exactly as today; behaviour of both unchanged. |
| A6 | At 960×640 the summary card's rect lies fully inside the Settings content scroll viewport with scroll offset 0. |
| A7 | If `account()` is `None` while state is Active/Expired, the signed-out view is shown (no placeholder text). |
| A8 | Search results `account.recheck_subscription` / `account.sign_out` highlight the respective button (F17–F19 apply). |

## Sign-out confirmation (FR-013, FR-014, SC-004)

| ID | Rule |
|---|---|
| S1 | `signout_dialog_width(w) = clamp(w − 64, 420, 560)` (64 = 2·`space::XXL`). Pure, public-in-crate, doc-tested: `960 → 560`, `500 → 436`, `400 → 420`. |
| S2 | Title, intro and every consequence item (one per `signout_categories()`) are wrapping labels — no truncation/elision (no `…` in any node name or painted galley). Bullets use a hanging indent. |
| S3 | At 960×640 with `with_pseudo_expansion(40)`, the modal's outer rect ⊆ viewport and every consequence node's bounds ⊆ modal rect; no internal scroll area. |
| S4 | On open, Cancel receives keyboard focus once; Enter/Space then activate Cancel (dialog closes, still signed in). |
| S5 | Escape and backdrop click close as Cancel; only an explicit Sign out activation calls `account.sign_out()`. |
| S6 | Button order Cancel, Sign out; Sign out uses `Variant::Destructive`. |

## Test map

| Test | Rules |
|---|---|
| `crates/modplayer-ui/src/settings/account.rs` unit tests | A3, A4 (`format_last_verified` with fixed `UtcOffset`s incl. day rollover, midnight `00:05`), S1 |
| `crates/modplayer-ui/tests/settings_account.rs` (new) | A1, A2, A5–A8, S2–S6 (fake `AccountService` via existing test fixtures used by `first_launch.rs`/`accessibility.rs`) |
| `crates/modplayer-ui/tests/accessibility.rs` (extend) | modal names/roles still pass sweep (T106) |
