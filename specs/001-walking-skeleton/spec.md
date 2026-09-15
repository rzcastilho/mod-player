# Feature Specification: Walking Skeleton — App Shell, Synthetic Source, and Audio Output

**Feature Branch**: `feature/001-walking-skeleton`

**Created**: 2026-09-14

**Status**: Clarified (2026-09-14)

**Input**: User description: "Build the runnable skeleton of ModPlayer, a desktop music player for musicians and DJs, so that a user on any of the three major desktop operating systems can launch the app, pick an output device, hear a test tone, and play a synthetic test track through a real-time audio engine. Nothing here talks to a streaming service yet; the point is a demonstrable, testable audio path and app shell that every later feature plugs into."

**Source**: [specs/autonomous/breakdown/001-mvp/001-walking-skeleton.md](../autonomous/breakdown/001-mvp/001-walking-skeleton.md) — traces to spec FR-1.3, FR-3.4, FR-6.1.3, FR-6.1.4, FR-11.1, FR-14.1, FR-14.4, DM-20, EC-3.8, EC-3.9, NFR §1, NFR §2, NFR §6, NFR §7, NFR §9, NFR §10, GOV §3.

**Prerequisites**: None — first slice of the MVP.

## Clarifications

### Session 2026-09-14

Resolution ladder applied: (D) derived from an authoritative source, (A) assumed conventional default. Every item below is also encoded in the requirement text; this section records the decision and its source or rationale.

- Q: What is the first-launch device-check flow, exactly? → A (D, FR-1.3.1, macro-spec §Prompt): On first launch (no confirmed device stored) the app shows the **Device Check** screen: output device list with the system default pre-selected, a buffer-preset selector, a "Play test tone" action, and the question "Did you hear that?" with **Yes** / **No, try another** / **Skip for now**. The tone plays automatically once when the screen appears, and again within one second of any of: selecting a different device, pressing "Play test tone", or pressing "No, try another". **Yes** marks the selected device confirmed and persists it; **Skip for now** closes the screen, leaves no confirmed device, and the screen returns on the next launch. The same screen is reachable on demand from Settings › Audio › "Test output device" (FR-1.3.1 "on demand from Settings").
- Q: What are the test tone's parameters and "capped safe level"? → A (A): 440 Hz sine, 1.0 s long, 10 ms linear fade-in and fade-out, rendered at a fixed −20 dBFS peak *before* the limiter, independent of master volume and of the safe-volume cap. It still passes through the non-bypassable limiter. Rationale: −20 dBFS/440 Hz is the conventional calibration tone; fixed level guarantees "capped safe" regardless of stored volume.
- Q: What are the buffer size presets, their labels, and the default? → A (D for the "performance" name, NFR-1.1; A for values): exactly three presets: **Performance** (128 frames), **Balanced** (256 frames), **Safe** (1024 frames). Each label shows the approximate one-way output latency computed from the negotiated frame count and the device's current sample rate, e.g. "Balanced (~5 ms)". Default on first launch: **Balanced**. If the platform cannot honour the requested size, the nearest supported size is used and the displayed latency reflects the size actually negotiated (FR-3.4.1 "where the platform allows"). Raw frame counts are shown only in Settings › Developer.
- Q: What does "one buffer duration" mean as a bound? → A (A): frames of the active preset as actually negotiated, divided by the device's current sample rate. All "within one buffer" criteria (device fallback, sample-rate change, buffer-size change) use this value.
- Q: Which volume control does "safe volume on startup" cap, and in what units? → A (A): this slice includes a **master volume** control (0–100 %, linear amplitude scale, dB equivalent shown alongside; 100 % = unity gain) applied in the engine before the limiter. Master volume persists across restarts. "Safe volume on startup" is a boolean plus a cap in the same 0–100 % units; **default: enabled, cap 50 %**. On launch, if enabled, the effective master volume is `min(stored volume, cap)`; after launch the user may raise it above the cap freely (the cap is a launch-time clamp, not a running ceiling). Rationale: FR-3.4.3 needs a volume to cap; percent is the conventional unit for a consumer volume fader; 50 % is a conservative, trivially changeable default.
- Q: Limiter default ceiling and out-of-range handling? → A (A): default ceiling **−1.0 dBFS**. Range −6.0 to −0.1 dBFS in 0.1 dB steps. Out-of-range values are **clamped** at both the UI (bounded control) and the engine command boundary (defence in depth); no code path can set the ceiling above −0.1 dBFS or disable the limiter. Rationale: −1 dBFS is the common distribution/streaming ceiling; clamping never leaves the system in an invalid state.
- Q: Limiter guarantee semantics? → A (A): **sample-peak brickwall**: no output sample magnitude may exceed the ceiling, measured at the limiter output before device sample-rate conversion. True-peak/inter-sample limiting and lookahead quality are deferred to a later slice; the guarantee here is correctness, not transparency.
- Q: Where does the user play the synthetic test track, and what is in it? → A (A): **Now Playing** in this slice is a minimal transport, not a placeholder: track title ("Synthetic test track"), Play/Pause, Stop, master volume, and a peak meter (dBFS, with the ceiling marked). The synthetic track is a deterministic, built-in, looping sequence of at least: 10 s 440 Hz sine at −12 dBFS (phase-continuity/dropout checks), 5 s 1 kHz square at 0 dBFS (proves the limiter engages), 5 s sawtooth sweep 100 Hz→2 kHz, 2 s silence. Rationale: the transport is the smallest surface that makes FR-3.4.3 (volume) and the limiter observable by a user without external tools; the fixture content is chosen so each acceptance test has a known signal.
- Q: Playback state at app launch? → A (D, NFR-2.6 "playback does not auto-resume"): the app always starts with transport **stopped**; the only audio that plays without a user transport action is the first-launch/on-demand test tone.
- Q: Synthetic source sample rate and engine internal rate? → A (D, FR-6.1.3; A for the value): the engine processes at the source's rate and converts to the device rate only in the output stage. The synthetic source runs at **44.1 kHz** by default (configurable in tests) so that the output-stage conversion is exercised on 48 kHz devices. When source and device rates match, the output stage passes samples through unchanged. Converter quality is not constrained in this slice.
- Q: Channel layout? → A (A): the engine is stereo. Mono devices receive `(L+R)/2`; devices with more than two channels receive L/R on channels 1–2 and silence elsewhere.
- Q: What happens when the device is lost and no fallback exists? → A (D, EC-3.8 "if none, pause"): fall back to the current system default; if no output device remains, transport enters **Paused** (position retained), and a critical notification is raised. The spec's earlier "stops audio safely" is realigned to "pauses".
- Q: Does the app switch back when the lost device reappears? → A (A): **No automatic switch-back** mid-session. The remembered device preference is retained; on the next launch, if the remembered device is present it is used. An info notification announces "<device> is available again" so the user can switch from Settings. Rationale: automatic device flips mid-performance are surprising; the conservative default is the reversible one.
- Q: Remembered device missing at launch? → A (D, FR-1.3.2): use the system default and raise a **warning** notification naming the missing device (not critical — nothing was interrupted; FR-14.1.1's "audio device lost" critical class is reserved for loss during playback). The first-launch prompt is not re-shown; preference is retained.
- Q: How are devices identified for persistence? → A (A): by the platform's stable device identifier when the platform provides one, otherwise by device name; matching on launch is by identifier first, then name.
- Q: Notification lifecycle? → A (A, FR-14.1.1/14.1.2, NFR-6.4): notifications stack newest-first in the notification area. **Critical** and **warning** persist until dismissed by the user; **info** auto-dismisses after 10 s and can also be dismissed manually. Each notification shows a severity icon and severity text (colour is never the only carrier). No notification is modal and none touches the audio path.
- Q: Is an explicit theme override in scope? → A (D, FR-14.4.1 "by default" implies an override exists; A for scope): Settings › Appearance exposes **Theme: System / Light / Dark**, default **System**. With System, the app tracks OS theme changes live. This is the only working control in Appearance in this slice.
- Q: Languages and string externalization in this slice? → A (D, Constitution X, NFR-7.1): all user-facing host strings are externalized from the first commit; **English (en-US) is the only shipped locale** in this slice. Settings › Language shows the locale selector with English as the only option; pt-BR translation lands in a later slice.
- Q: Accessibility and keyboard operability? → A (D, Constitution X, NFR-6.1/6.2): every control introduced in this slice (navigation, device check, transport, notifications, settings and search) is operable by keyboard alone and exposes an accessible name, role and state to the platform's assistive technology. Focus order follows visual order.
- Q: Settings search semantics? → A (A; NFR-1.12 for timing): case-insensitive substring match against each setting's title and description, live per keystroke, results rendered within 50 ms of the keystroke, each result showing its category path, and selecting a result navigates to that setting in its category. Placeholder categories contribute no searchable settings until they have real ones.
- Q: Settings persistence guarantees? → A (D, NFR-2.8 convention; A for scope): settings are written to the platform's per-user configuration location using an atomic replace (write-then-rename), so a crash mid-write never corrupts previously saved settings. Persisted fields in this slice are exactly DM-20's subset relevant here: output device identity, buffer preset, limiter ceiling, safe-volume enabled + cap, master volume, theme, plus a "device confirmed" flag.
- Q: What must CI gate on, and how are audio tests run without hardware? → A (D, Constitution VII/VIII, GOV-3.3): CI runs on macOS, Windows and Linux for every change and gates merges on `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, and a license-header check. Engine tests run against an in-process test output sink (no physical device) so they pass on headless runners; tests that need physical device removal or an external sample-rate change are marked manual and are excluded from CI gating.
- Q: How is real-time safety verified, not just asserted? → A (D, Constitution I/VIII): automated tests must (a) fail if the audio callback allocates (counting/panicking allocator installed around the callback under test), (b) prove that a parameter change enqueued mid-buffer takes effect exactly at the next buffer boundary, and (c) prove the audio clock is monotonic and advances by exactly the frames rendered across a device fallback and a sample-rate change.
- Q: What is the "monotonic audio clock"? → A (D, FR-6.1.4, glossary "Audio clock"): a sample-frame counter owned by the engine, advanced only by the real-time path by the number of frames rendered, never rewound or reset while the app runs (device fallback and sample-rate change do not reset it), and readable off the real-time path via an atomic.
- Q: Zero output devices at launch? → A (D, edge case; A for behaviour): Device Check shows an empty-state message and only "Skip for now"; no tone is attempted; a critical notification is raised; Now Playing's transport is disabled with an inline reason. When a device appears later, an info notification announces it and the transport re-enables.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - First-launch device check and test tone (Priority: P1)

A user launches ModPlayer for the first time. The Device Check screen lists available output devices, pre-selects the system default, plays a short test tone at a fixed safe level, and asks "Did you hear that?" The user confirms, picks a different device and re-tests, or skips.

**Why this priority**: This is the walking skeleton's core proof: a real, audible signal travels from the app through a real audio device on the user's machine. Without this, nothing else in the product can be trusted to make sound. It is also the very first thing every new user sees.

**Independent Test**: Fresh install, no prior configuration. Launch the app, observe the device list and default selection, confirm the tone plays and is audible, and verify the chosen device persists across a restart. Delivers standalone value: proof the app can talk to the machine's audio hardware.

**Acceptance Scenarios**:

1. **Given** a fresh install with no stored preferences, **When** the app launches, **Then** the Device Check screen lists the system's output devices, pre-selects the system default, shows the buffer preset selector defaulting to Balanced, and plays the 440 Hz / 1 s / −20 dBFS test tone automatically once.
2. **Given** the Device Check screen is showing, **When** the user selects a different device, presses "Play test tone", or presses "No, try another", **Then** the test tone starts on the currently selected device within one second, at the same fixed −20 dBFS level.
3. **Given** the Device Check screen is showing, **When** the user answers **Yes**, **Then** the selected device and buffer preset are persisted, the device is marked confirmed, and the main window opens with transport stopped.
4. **Given** the user confirmed a device during first launch, **When** the app is closed and reopened, **Then** the previously confirmed device is used automatically and the Device Check screen is not shown.
5. **Given** the Device Check screen is showing, **When** the user presses **Skip for now**, **Then** the main window opens, no device is marked confirmed, and the Device Check screen is shown again on the next launch.
6. **Given** the buffer preset selector is showing, **When** the user opens it, **Then** exactly three presets — Performance, Balanced, Safe — are offered, each labelled with its approximate latency in milliseconds for the selected device's current sample rate, and no raw frame counts are shown.
7. **Given** the main window is open, **When** the user opens Settings › Audio › "Test output device", **Then** the same Device Check screen appears and behaves identically.

---

### User Story 2 - Continuous synthetic playback through the real-time engine and limiter (Priority: P2)

A user opens Now Playing, presses Play on the built-in synthetic test track, and hears continuous, glitch-free audio. Output level never exceeds the user-set limiter ceiling, enforced by a limiter that cannot be turned off; master volume and the safe-startup cap behave as documented.

**Why this priority**: This proves the actual real-time engine — not just a one-shot tone — stays glitch-free over time and that the non-bypassable safety limiter genuinely protects the user's ears and equipment. It is the second pillar of the walking skeleton after "can we make any sound at all."

**Independent Test**: Start playback of the built-in synthetic test track, let it run through at least two full loops, and verify (by the on-screen peak meter and by an automated sink-level test) that audio never exceeds the configured ceiling and playback has no dropouts under normal conditions. Delivers standalone value: a trustworthy, safety-bounded playback path.

**Acceptance Scenarios**:

1. **Given** the synthetic test track is playing and reaches its 0 dBFS square segment, **When** the user sets the ceiling to any value between −6.0 dBFS and −0.1 dBFS, **Then** no output sample at the limiter output exceeds that ceiling, and the peak meter never reads above it.
2. **Given** "safe volume on startup" is enabled with cap C and master volume was saved at V > C, **When** the app launches, **Then** master volume is C; **When** the user then raises master volume above C, **Then** the new value is applied and persisted, and the next launch again clamps to C.
3. **Given** "safe volume on startup" is disabled, **When** the app launches, **Then** master volume is the saved value V unchanged.
4. **Given** playback is running, **When** any part of the app changes master volume, ceiling, buffer preset, or output device, **Then** the change is applied at the next buffer boundary (never mid-buffer) and causes no audible glitch or clock discontinuity.
5. **Given** the synthetic source is producing the 440 Hz sine segment, **When** playback runs continuously for at least 60 s, **Then** the automated sink test observes no dropped or duplicated frames, and the audio clock advances by exactly the number of frames rendered.
6. **Given** the app launched with a confirmed device, **When** the main window appears, **Then** the transport is stopped and no audio plays until the user presses Play.

---

### User Story 3 - Output device resilience (Priority: P3)

While synthetic audio is playing, the user's chosen output device disappears (e.g., unplugged) or changes its sample rate. Playback continues with minimal, bounded interruption, and the user is informed.

**Why this priority**: Musicians and DJs work with real, sometimes flaky, hardware (USB interfaces, Bluetooth devices) during live use. Silent failure or a long dropout is unacceptable; this story proves the app degrades gracefully instead of going silent or crashing.

**Independent Test**: Start playback, then physically or programmatically remove the active output device (or trigger a sample-rate change on it). Observe that audio continues on the fallback device within one buffer duration and that a notification names the lost device. Delivers standalone value: confidence the app survives real-world hardware changes mid-session.

**Acceptance Scenarios**:

1. **Given** synthetic audio is playing on a chosen device, **When** that device disappears, **Then** output resumes on the current system default device within one buffer duration, a critical notification names the lost device, and the audio clock is not reset.
2. **Given** synthetic audio is playing, **When** the active device's sample rate changes externally, **Then** only the output stage reconfigures (source-rate processing and the clock are unaffected) and the audible gap is at most one buffer duration.
3. **Given** the active device disappears and no output device remains, **When** fallback is attempted, **Then** transport enters Paused with position retained, a critical notification is raised, and the app does not crash.
4. **Given** playback fell back to the system default after device loss, **When** the lost device reappears, **Then** output stays on the fallback device, an info notification announces the device is available again, and the stored preference still names the original device.
5. **Given** the remembered device is absent at launch, **When** the app starts, **Then** the system default is used, a warning notification names the missing device, and the Device Check screen is not shown.

---

### User Story 4 - App shell navigation and notifications (Priority: P4)

A user sees a main window with navigation for Library, Now Playing, Plugins, and Settings (placeholder content for Library and Plugins; Now Playing hosts the minimal transport), a non-blocking notification area, and a light/dark theme that follows the system setting by default.

**Why this priority**: Every later feature slice plugs into this shell. It carries no audio risk on its own, so it is lower priority than the audio-path stories, but it must exist for the walking skeleton to look and feel like the eventual product.

**Independent Test**: Launch the app, navigate between the four sections by mouse and by keyboard alone, trigger a sample notification of each severity (critical/warning/info) from Settings › Developer, and switch the OS theme to confirm the app follows it. Delivers standalone value: a navigable shell that doesn't block on any single section's content.

**Acceptance Scenarios**:

1. **Given** the app is open, **When** the user selects Library, Now Playing, Plugins, or Settings from the navigation (by pointer or keyboard), **Then** the corresponding section is shown; Library and Plugins may show placeholder content.
2. **Given** a critical, warning, or info-level event occurs during playback, **When** it is raised, **Then** a non-blocking notification appears newest-first in the notification area with a severity icon and severity text, and playback and navigation are not interrupted.
3. **Given** an info notification is showing, **When** 10 s elapse, **Then** it dismisses itself; **Given** a critical or warning notification is showing, **When** 10 s elapse, **Then** it remains until the user dismisses it.
4. **Given** Theme is set to System and the OS is in dark (or light) mode, **When** the app launches or the OS theme changes while the app is running, **Then** the app's theme matches the system setting without restart.
5. **Given** Theme is set to Light or Dark, **When** the OS theme changes, **Then** the app's theme does not change.
6. **Given** any screen in this slice, **When** an assistive-technology inspector is used, **Then** every interactive element exposes an accessible name, role and state, and every action is reachable by keyboard alone.

---

### User Story 5 - Searchable, organized Settings (Priority: P5)

A user opens Settings and finds it organized into named categories (Account, Audio, Playback, Controls, Plugins, Offline, Appearance, Language, Developer, Privacy & diagnostics, About) and can search across them.

**Why this priority**: Settings drives the device/buffer choices from User Story 1 and the limiter ceiling from User Story 2, but the full categorized, searchable structure is a usability layer that can land after the core audio path is proven. Most categories show placeholder content in this slice; Audio, Appearance, Language and Developer have the working controls listed in FR-018.

**Independent Test**: Open Settings, confirm all eleven categories are listed and navigable, and search for "ceiling" to confirm the limiter setting surfaces with its category path. Delivers standalone value: a discoverable settings surface for every later feature to extend.

**Acceptance Scenarios**:

1. **Given** Settings is open, **When** the user views the category list, **Then** all eleven categories (Account, Audio, Playback, Controls, Plugins, Offline, Appearance, Language, Developer, Privacy & diagnostics, About) are present and selectable in that order.
2. **Given** Settings is open on any category, **When** the user types a term that matches a setting's title or description (case-insensitive substring), **Then** matching settings appear within 50 ms of the keystroke with their category path, and selecting one navigates to that setting.
3. **Given** the Audio category is open, **When** the user changes output device, buffer preset, limiter ceiling, safe-volume-on-startup (enabled / cap), or presses "Test output device", **Then** the change takes effect at the next buffer boundary and persists across restarts (the test action opens the Device Check screen).
4. **Given** the Developer category is open, **When** the user presses "Raise sample notification" for a severity, **Then** a notification of that severity appears in the notification area.

---

### Edge Cases

- System reports zero output devices at launch: Device Check shows an empty state with only "Skip for now"; no tone is attempted; a critical notification is raised; Now Playing's transport is disabled with an inline reason; when a device appears, an info notification announces it and the transport re-enables. The app MUST NOT crash.
- User answers "No, try another" across every available device: the user can always press "Skip for now"; no device is marked confirmed; Device Check returns on the next launch.
- Buffer preset changes while audio is playing: applied at a buffer boundary with at most one buffer's gap, same as a sample-rate change.
- Platform refuses the requested buffer frame count: nearest supported size is used and the preset label shows the latency actually negotiated.
- OS theme changes mid-session with Theme = System: app follows immediately. With Theme = Light/Dark: app ignores the OS change.
- A future plugin attempts to touch the real-time path directly: out of scope for this slice's implementation, but the boundary-applied command queue introduced here MUST already be the only write path into the engine, so later plugin work has nothing else to plug into.
- Ceiling set outside −6.0 to −0.1 dBFS (via UI, settings file edit, or any command): clamped into range; the limiter is never bypassable regardless of input.
- Safe-volume cap set above 100 % or below 0 %: clamped to 0–100 %.
- Settings file is missing or unparseable at launch: defaults are used (Balanced, −1.0 dBFS, safe volume enabled at 50 %, master volume 80 %, Theme System, no confirmed device), a warning notification is raised, and the file is rewritten on the next settings change.
- App crashes mid-write of settings: previously saved settings remain intact (atomic replace).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The app MUST run on macOS, Windows, and Linux with identical navigation, keyboard behavior, and audio behavior across all three; platform differences are confined to adapter code. *(Traces: NFR §9 Compatibility and portability; Constitution Principle X)*
- **FR-002**: On first launch (no confirmed device stored) and on demand from Settings › Audio › "Test output device", the app MUST show the Device Check screen: the enumerated output devices with the system default pre-selected, the buffer preset selector, a "Play test tone" action, and the prompt "Did you hear that?" with **Yes** / **No, try another** / **Skip for now**. The test tone MUST play automatically once when the screen appears. *(Traces: FR-1.3.1 Audio device check)*
- **FR-003**: Selecting a different device, pressing "Play test tone", or pressing "No, try another" MUST start the test tone on the currently selected device within one second. The test tone is a 440 Hz sine, 1.0 s, 10 ms fade-in/out, fixed at −20 dBFS peak before the limiter, independent of master volume and the safe-volume cap.
- **FR-004**: Answering **Yes** MUST persist the selected device and buffer preset and mark the device confirmed; subsequent launches MUST use it and MUST NOT show the Device Check screen. **Skip for now** MUST leave no device confirmed so that the screen returns on the next launch. Devices are identified by the platform's stable identifier when available, else by name; matching is by identifier first, then name. *(Traces: FR-1.3.2)*
- **FR-005**: The buffer preset selector MUST offer exactly three presets — Performance (128 frames), Balanced (256 frames), Safe (1024 frames) — labelled with the approximate latency in milliseconds computed from the negotiated frame count and the device's current sample rate; raw frame counts MUST NOT appear outside Settings › Developer. Default is Balanced. If the platform cannot honour a size, the nearest supported size MUST be used and the label MUST reflect it. *(Traces: FR-3.4.1)*
- **FR-006**: The audio engine MUST run on a dedicated real-time path that owns a monotonic audio clock (a frame counter advanced only by frames rendered, never reset or rewound while the app runs, readable off-path via an atomic) and MUST NOT allocate, lock, block, log, or perform I/O within its callback. The engine MUST process at the source sample rate and convert to the device rate only in the output stage. *(Traces: Part 9 §1 Guiding decisions, Part 9 §3 AR-3, FR-6.1.3, FR-6.1.4; Constitution Principle I)*
- **FR-007**: Any component that needs to change real-time-path state (master volume, ceiling, device, buffer preset, transport) MUST do so only through a lock-free command queue whose entries are applied at a buffer boundary, never mid-buffer; there MUST be no other write path into the engine. *(Traces: Part 9 §4 Communication patterns; Constitution Principle I)*
- **FR-008**: Audio MUST be produced by a swappable Audio Source behind a common interface; this slice MUST ship only a synthetic implementation. The synthetic source MUST run at 44.1 kHz by default (configurable in tests) and MUST provide (a) the test tone and (b) a deterministic looping test track of at least: 10 s 440 Hz sine at −12 dBFS, 5 s 1 kHz square at 0 dBFS, 5 s sawtooth sweep 100 Hz→2 kHz, 2 s silence. No other crate may depend on a future streaming source. *(Traces: Part 9 §1 decision 2, AR-1; Constitution Principle IV)*
- **FR-009**: The app MUST apply a hard output limiter after the engine (and after master volume) that cannot be bypassed or disabled by any setting, command, or code path. The limiter MUST guarantee that no output sample magnitude exceeds the ceiling at the limiter output, before device sample-rate conversion. *(Traces: FR-3.4.2, NFR-6.8; AR-5)*
- **FR-010**: The limiter ceiling MUST be user-settable from −6.0 dBFS to −0.1 dBFS in 0.1 dB steps, default −1.0 dBFS. Values outside the range from any source (UI, settings file, command) MUST be clamped into range at both the UI and the engine command boundary.
- **FR-011**: The app MUST provide a master volume control (0–100 %, linear amplitude, dB equivalent displayed, 100 % = unity) applied before the limiter and persisted across restarts. The app MUST provide a "safe volume on startup" option (boolean + cap, 0–100 %), default enabled with cap 50 %. When enabled, the effective master volume at launch MUST be `min(stored volume, cap)`; the user MAY raise it above the cap afterwards. *(Traces: FR-3.4.3, NFR-6.8, DM-20)*
- **FR-012**: When the active output device becomes unavailable during playback, the app MUST fall back to the current system default device within one buffer duration, MUST raise a critical notification naming the lost device, and MUST NOT reset the audio clock. If no output device remains, transport MUST enter Paused with position retained and a critical notification MUST be raised. The app MUST NOT automatically switch back when the lost device reappears; it MUST raise an info notification that the device is available again and MUST retain the stored preference. *(Traces: FR-1.3.2, EC-3.8, NFR-2.4)*
- **FR-013**: When the active output device's sample rate changes, only the output stage MUST reconfigure; source-rate processing and the audio clock MUST be unaffected and the audible gap MUST be at most one buffer duration. The same bound applies to a buffer preset change during playback. "One buffer duration" is the negotiated frame count of the active preset divided by the device's current sample rate. *(Traces: EC-3.9)*
- **FR-014**: When the remembered device is absent at launch, the app MUST use the system default, MUST raise a warning notification naming the missing device, MUST retain the stored preference, and MUST NOT re-show the Device Check screen.
- **FR-015**: The app MUST present a main window with navigation for Library, Now Playing, Plugins, and Settings. Library and Plugins MAY show placeholder content. Now Playing MUST show the track title, Play/Pause, Stop, master volume, and a peak meter in dBFS with the ceiling marked. The app MUST start with transport stopped; the only audio that plays without a transport action is the test tone. *(Traces: AR-19; NFR-2.6)*
- **FR-016**: The app MUST provide a non-blocking notification area in which notifications stack newest-first and are classified critical, warning, or info, each showing a severity icon and severity text. Critical and warning notifications MUST persist until dismissed; info notifications MUST auto-dismiss after 10 s and MAY be dismissed manually. No notification MAY be modal or interrupt playback or navigation. *(Traces: FR-14.1.1, FR-14.1.2, NFR-6.4)*
- **FR-017**: The app MUST support light and dark themes. Settings › Appearance MUST expose Theme = System / Light / Dark, default System; with System the app MUST follow OS theme changes live without restart. *(Traces: FR-14.4.1)*
- **FR-018**: Settings MUST be organized into the categories Account, Audio, Playback, Controls, Plugins, Offline, Appearance, Language, Developer, Privacy & diagnostics, About, in that order. Working controls in this slice: **Audio** — output device, buffer preset, limiter ceiling, safe volume on startup (enabled, cap), "Test output device"; **Appearance** — Theme; **Language** — locale selector (English only); **Developer** — raw buffer frame count readout, "Raise sample notification" (critical / warning / info). All other categories MAY show placeholder content. *(Traces: FR-11.1.1)*
- **FR-019**: Settings MUST be searchable across all categories: case-insensitive substring match on setting title and description, results within 50 ms of each keystroke, each showing its category path, and selecting a result navigates to that setting. *(Traces: FR-11.1.2, NFR-1.12)*
- **FR-020**: Settings (output device identity, buffer preset, limiter ceiling, safe-volume enabled + cap, master volume, theme, device-confirmed flag) MUST persist in the platform's per-user configuration location via atomic replace, so a crash mid-write never corrupts previously saved settings. A missing or unparseable settings file MUST result in defaults plus a warning notification, never a crash. *(Traces: DM-20, NFR-2.8 convention)*
- **FR-021**: All user-facing host strings MUST be externalized from the first commit; English (en-US) is the only shipped locale in this slice. *(Traces: NFR-7.1; Constitution Principle X)*
- **FR-022**: Every interactive element introduced in this slice MUST be operable by keyboard alone and MUST expose an accessible name, role, and state to the platform's assistive technologies; focus order MUST follow visual order. *(Traces: NFR-6.1, NFR-6.2; Constitution Principle X)*
- **FR-023**: The engine MUST be stereo. Mono devices MUST receive `(L+R)/2`; devices with more than two channels MUST receive L/R on channels 1–2 and silence elsewhere.
- **FR-024**: Continuous integration MUST build and test on macOS, Windows, and Linux from the project's first commit and MUST gate every merge on `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo deny check`, and a license-header check. Engine tests MUST run against an in-process test output sink so they pass on headless runners; tests requiring physical device removal or external sample-rate change MUST be marked manual and excluded from CI gating. *(Traces: GOV-3.3; Constitution Principles VII, VIII)*
- **FR-025**: The automated test suite MUST include: (a) a test that fails if the audio callback allocates; (b) a test proving a command enqueued mid-buffer takes effect exactly at the next buffer boundary; (c) a test proving the audio clock is monotonic and advances by exactly the frames rendered across a simulated device fallback and a simulated sample-rate change; (d) a test proving no sample at the limiter output exceeds the ceiling for every ceiling in the range when fed the 0 dBFS square segment; (e) a test proving clamping of out-of-range ceiling and cap values. *(Traces: Constitution Principle VIII)*
- **FR-026**: The app MUST NOT include any sign-in flow, streaming catalog browsing, waveform display, plugin execution, effect nodes, or any capability that writes decoded audio to a user-accessible file or exposes sample buffers outside the engine. *(Scope boundary; Constitution Principle V)*

### Key Entities

- **Audio Source**: A replaceable, isolated producer of stereo audio frames at its own sample rate behind a common interface. This slice ships one implementation — the Synthetic Audio Source (44.1 kHz default) — which generates the test tone and the deterministic looping test track without any external service.
- **Output Device**: A system-reported audio destination with a stable identifier (or name), a current sample rate, a negotiated buffer frame count, and availability status; one device is active at a time, with a remembered user preference (plus confirmed flag) and a system-default fallback.
- **Real-Time Audio Engine**: The dedicated processing path that owns the monotonic audio clock (frame counter), pulls from the Audio Source at source rate, applies master volume, feeds the Output Limiter, and hands frames to the output stage; accepts state changes only via the boundary-applied command queue and publishes clock and peak level via atomics/queues.
- **Output Limiter**: A non-bypassable sample-peak brickwall stage after master volume enforcing the user-set ceiling (−6.0 to −0.1 dBFS, default −1.0), before device sample-rate conversion.
- **Output Stage**: Converts from source rate to the device rate (passthrough when equal), maps stereo to the device's channel count, delivers to the platform audio path, and handles device loss and sample-rate change with bounded gaps.
- **Audio Settings (DM-20 subset)**: Output device identity, device-confirmed flag, buffer preset, limiter ceiling, safe-volume enabled + cap, master volume; persisted atomically alongside Theme.
- **Notification**: A user-facing message with severity (critical / warning / info), icon, text, and lifecycle (persistent until dismissed for critical/warning; 10 s auto-dismiss for info), shown newest-first in a non-blocking area.
- **Settings Category**: One of eleven ordered groupings organizing searchable settings; each setting has a title and description used by search.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a fresh launch, 100% of manual trials show the test tone playing automatically when Device Check appears, and starting within one second of any re-test action.
- **SC-002**: When the active output device is removed during playback, audio resumes on the system default within one buffer duration in 100% of trials, a critical notification naming the lost device appears within one second, and the audio clock shows no reset.
- **SC-003**: When the active device's sample rate or the buffer preset changes mid-playback, the gap is at most one buffer duration in 100% of trials, with no engine restart and no clock reset.
- **SC-004**: The automated limiter test (FR-025 d) passes for every ceiling value in −6.0..−0.1 dBFS in 0.1 dB steps, and the on-screen peak meter never reads above the ceiling during the 0 dBFS square segment in 100% of manual trials.
- **SC-005**: CI builds and passes all FR-024 gates on macOS, Windows, and Linux from the first commit onward; 100% of merges are gated on all three platforms passing.
- **SC-006**: The automated real-time tests (FR-025 a–c, e) exist and pass on all three CI platforms.
- **SC-007**: Users can navigate to any of the four main sections, by pointer and by keyboard alone, and observe a notification of any severity without playback audibly interrupting, in 100% of manual trials.
- **SC-008**: Searching "ceiling" in Settings from any category surfaces the limiter ceiling setting with its category path within 50 ms of the last keystroke; users locate it in under 10 seconds without knowing its category.
- **SC-009**: With safe volume enabled at cap C and stored volume V > C, 100% of launches start at master volume C.

## Assumptions

- Target users run one of macOS, Windows, or Linux on desktop hardware with at least one functioning audio output device; mobile and web are out of scope.
- Default values in this slice: buffer preset Balanced (256 frames); limiter ceiling −1.0 dBFS; safe volume on startup enabled with cap 50 %; master volume 80 %; Theme System; locale en-US. All are user-changeable and are documented under Clarifications as assumed defaults.
- The synthetic test track and test tone are built-in fixtures generated at runtime; they require no network access and no external asset download.
- Reference hardware list and reference track set (A-16) are not required for this slice; every acceptance criterion here is verifiable with any standard system audio device plus the in-process test sink. NFR-1.13 (start-to-window ≤ 3 s) is therefore not gated in this slice.
- Maximum acceptable app footprint (Q-15) is not constrained in this slice; it remains open for a later slice.
- No user accounts, credentials, or streaming connectivity exist yet; Account and any sign-in-shaped UI are placeholder-only.
- Sample-rate converter quality and true-peak limiting are deferred; this slice guarantees correctness bounds (sample-peak ceiling, bounded gaps), not audio transparency.
- Technology choices (UI toolkit, audio backend, settings file format) are made in `plan.md` within the constitution's constraints (Rust, one crate per component, `#![forbid(unsafe_code)]` outside FFI crates).
