# 010-now-playing-workbench / 001 — Sticky Transport Bar and Panel Layout

**Source:** [§ 3.5 Now Playing](../../ModPlayer-UI-UX-Review.md#35-now-playing) (UX-21, UX-22, UX-26), [§ 5.5 Layout rules](../../ModPlayer-UI-UX-Review.md#55-layout-rules), [§ 5.4 Component rules](../../ModPlayer-UI-UX-Review.md#54-component-rules), [§ 1 Summary](../../ModPlayer-UI-UX-Review.md#1-summary)

**Prerequisites:** Requires the panel card and toggle components from 008-design-foundation/002-control-variants and 003-list-row-and-panel-components, and the window sizing from 009-window-and-shell/001-window-sizing-and-responsive-dock.

## Prompt

> Reorganise the now-playing view around the control a musician touches every few seconds. Today it is a single undifferentiated column of roughly fourteen controls — heading, transport, three toggles, waveform, marker lane, markers, effect chain, transport focus, volume, meter, queue — every block separated by the same hairline, and the transport scrolls out of sight as soon as a panel is opened.

> Split the view into three regions. A transport bar pinned to the top carries the artwork thumbnail, the track title and artist, the play/pause, stop and skip controls, the elapsed and remaining times, the master volume with its peak meter, and the panel toggles; it never scrolls away. The waveform occupies the flexible middle and grows with the window. Everything else — markers, effect chain, transport focus, queue — becomes a collapsible panel card below the waveform, each with its own header, remembered open state, and space rather than a rule between it and its neighbour.

> Make the toggles behave like navigation into those panels: pressing Queue, Effects or Transport opens the matching panel and scrolls it into view, and pressing it again collapses it, so a toggle never appears to do nothing because its panel opened below the fold. The queue panel itself gains the shared row component — artwork, title over artist, right-aligned position — with the currently playing entry marked on the row rather than prefixed with the words "Now playing:", and its move, play-next and remove actions rendered as quiet row actions.

> Acceptance: when a panel is opened and the content is scrolled to the bottom, the transport controls remain visible and usable. When the Queue toggle is pressed, the queue panel opens and becomes visible without further scrolling. When the window is made taller, the waveform grows while the transport bar keeps its height. When the queue is displayed, the playing entry is identifiable without reading its label.

## Scope boundary

Covers the layout and the queue panel's rows; the contents of the markers, effect chain and waveform surfaces are restructured in the following files of this wave.
