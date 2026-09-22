# 011-browse-and-manage-polish / 003 — Plugins List as a Real Table

**Source:** [§ 3.6 Plugins](../../ModPlayer-UI-UX-Review.md#36-plugins) (UX-27, UX-28, UX-29), [§ 4.6 Accessibility versus the spec](../../ModPlayer-UI-UX-Review.md#46-accessibility-versus-the-spec), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules)

**Prerequisites:** Assumes the plugin list from 001-mvp/009-plugin-runtime-and-permissions and the panel and row components from 008-design-foundation/003-list-row-and-panel-components. Permission granting and revoking arrive with 005-community-registry/002-permission-management-and-usage-log.

## Prompt

> Make the plugins screen readable when more than two plugins are installed. Its column headers are a row of labels that align with nothing below them, and the permissions cell is a comma-joined sentence that runs off the right edge of even a wide window; with a dozen plugins loaded the screen is unusable.

> Lay the list out as a real table with aligned columns — name and version, source, enabled, health, permissions, and resource use — where every cell stays within its column and long values truncate with the full value available on hover. The permissions column becomes a count with a control that expands the plugin's granted permissions as a readable list rather than a run-on sentence. Resource use shows the plugin's CPU share and memory in tabular figures against their budgets, so a figure means something without knowing the budget by heart.

> Health becomes a recognisable state word — healthy, degraded, suspended — with its colour taken from the shared tokens rather than from literals in the view, keeping the rule that colour is never the only carrier of the state. A row's own controls — show or hide its panels, enable or disable it, restart it when suspended — belong to the row rather than sitting on a second indented line under it, and a suspended plugin's row says why it was suspended.

> Acceptance: when seventeen plugins are installed, every column stays within the window at the minimum width and no cell overlaps another. When a plugin's permission count is expanded, its permissions are listed one per line. When a plugin is suspended, its row shows the suspended state in words and colour and offers a restart action. When the source tree is searched for colour literals outside the theme module, the plugin health colours are no longer among them.

## Scope boundary

Covers the plugins list's layout and health presentation; granting, revoking and auditing permissions remain with 005-community-registry/002-permission-management-and-usage-log, and the plugin panel and floated-window chrome with 001-mvp/011-plugin-ui-contributions.
