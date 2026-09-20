# Section Loop

The reference bundled plugin (`org.modplayer.section-loop`): drop A and
B around a passage — a solo, a phrase, a bar — and drill it hands-free,
gaplessly, until it's solid. It ships with ModPlayer, enabled by
default, and is written only against the public plugin API, so its
source (`main.luau`) is also valid, unmodified example code for anyone
writing their own plugin.

## What it does

- **Set A / Set B** — panel buttons (and, once you resolve the default
  keyboard conflict below, `I` / `O`) drop the loop's start and end at
  the current playhead. Setting only one first is fine — an A-only
  region is valid and visible right away.
- **Loop** — the panel's Loop toggle (or `L`) arms/disarms gapless
  playback between A and B. Arming requests transport focus first, so
  it also works when another plugin currently holds it (under the
  default focus policy).
- **Repeat count** — the panel's slider sets how many times the loop
  wraps before it releases on its own; `0` means infinite (the
  default).
- **Nudge** — `[` / `]` move whichever of A or B you touched most
  recently by 10 ms, for fine-tuning a seam by ear.
- **Cues** — `set_cue_1`…`8` drop a numbered, named cue point at the
  playhead; `jump_cue_1`…`8` seek straight to one. Both ship unbound by
  default and are rebindable like any other action.
- **Clear markers** — removes every marker Section Loop owns (A, B,
  its cues) in one action; nothing else on the track is touched.
- **Snap to beat** — reserved for a later update (beat analysis isn't
  shipped yet); the toggle always shows off and does nothing.
- **Overlays** — A/B lines, the loop region, and cue dots/labels are
  drawn on the waveform, alongside the host's own native marker
  rendering.

Every control is reachable and operable by keyboard alone, with an
accessible name (contracts/section-loop-plugin.md, spec.md SC-006).

## How it uses the API

Section Loop is built only on `api.*` — the same surface any
third-party plugin sees, never a private host interface. It declares:
`playback.observe`, `transport.control`, `markers.read`,
`markers.write`, `ui.panel`, `ui.overlay`, `ui.shortcuts` (required),
and `analysis.read` (optional, reserved and unused this release).

It creates and moves its A/B endpoints and repeat count through two
plugin-API 1.3 calls — `markers.set_loop_endpoint` and
`markers.set_loop_repeat` — and reads them back (and every cue, of any
owner) through `markers.list()`. It never counts loop wraps or seam
timing itself: that stays entirely on the host's real-time audio path
(spec.md FR-009). See `contracts/plugin-api-v1.3.md` and
`contracts/section-loop-plugin.md` in the feature's spec folder for
the full contract this script honours.

## Persistence guarantee

A, B and every cue are ordinary markers in the **host's** per-track
marker store — not anything Section Loop keeps itself. That means:

- They survive closing and reopening ModPlayer: the same track shows
  the same A, B and cues, at their exact original positions, every
  time.
- They survive Section Loop being **disabled**: the loop releases and
  transport focus returns to the host immediately, but A, B and the
  cues stay right where they are on the waveform, still editable from
  the host's own Markers panel — no need to re-enable the plugin.
- They're visible to any other plugin holding `markers.read`.

Section Loop's own script state — which endpoint you touched last, an
un-applied repeat value typed before a region exists — is
session-only and is never persisted; it starts fresh every time the
plugin (re)starts, and the very next `markers.list()` rebuilds
everything else from the host's own record.

## A note on the default `I` / `O` / `L` keys

The host already binds `I` (set A), `O` (set B) and `L` (toggle loop)
as its own default actions (006). Section Loop registers the same
three keys as its own defaults, per its default-shortcut list — but on
a fresh install, the host's bindings win the conflict, and Section
Loop's identical ones are flagged inactive until you rebind one side in
Settings › Controls. This is expected: the panel's own Set A / Set B /
Loop controls work regardless, from the very first launch, and act on
Section Loop's own region — never on whatever the host's keys created.
