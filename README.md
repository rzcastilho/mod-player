# ModPlayer

A pluggable desktop music client for the Spotify catalog, built for
musicians who practise along to recordings: section looping, markers and
cues, key transpose and tempo stretch, a waveform view, keyboard-driven
transport — and a sandboxed Luau plugin runtime to extend all of it.

ModPlayer is a Rust workspace: `eframe`/`egui` UI, `cpal` audio output,
`librespot` as a Spotify Connect receiver, `mlua` (Luau) for plugins.
The product specification lives in
[`docs/ModPlayer-Software-Specification.md`](docs/ModPlayer-Software-Specification.md);
per-feature specs, plans and manual test scenarios live under
[`specs/`](specs/).

> **Spotify Premium is required.** ModPlayer streams through Spotify
> Connect, which Spotify only offers to Premium accounts. You sign in with
> your own account on first launch; the token is kept in the OS secure
> store (macOS Keychain, Windows Credential Manager, Secret Service on
> Linux).

## Quickstart

### 1. Prerequisites

- **Rust 1.95.0** — pinned in [`rust-toolchain.toml`](rust-toolchain.toml);
  `rustup` picks it up automatically. If your shell exports
  `RUSTUP_TOOLCHAIN`, that wins over the pin — either unset it or use
  `cargo +1.95.0 …`.
- **macOS / Windows:** nothing else.
- **Linux:** `libasound2-dev libxkbcommon-dev libwayland-dev pkg-config`
  (Debian/Ubuntu names).
- A C++ toolchain (Xcode Command Line Tools on macOS, MSVC on Windows,
  `build-essential` on Linux) — the Luau VM is compiled from source.

### 2. Build

```sh
git clone https://github.com/rzcastilho/mod-player.git
cd mod-player
cargo build --release -p modplayer
```

The first build takes a while (Luau, librespot, wgpu). The binary is
`target/release/modplayer`.

### 3. Run

**Linux / Windows:**

```sh
target/release/modplayer
```

**macOS — launch through an `.app` bundle:**

```sh
scripts/bundle-macos.sh          # wraps target/release/modplayer in target/release/ModPlayer.app
open target/release/ModPlayer.app
```

Rebuild + relaunch in one line:

```sh
cargo build --release -p modplayer && scripts/bundle-macos.sh && open target/release/ModPlayer.app
```

#### Why the bundle on macOS?

Running the bare binary (`./target/release/modplayer`) from a terminal —
in particular from a shell **inside tmux** — produces a window you can see
but cannot type into: keystrokes keep landing in the terminal, even after
clicking the window. The process is drawn by the window server but never
registers with Launch Services as a foreground application, so macOS will
not make it active (`System Events` doesn't even list it, and
`reattach-to-user-namespace` does not help).

Launching via `open …/ModPlayer.app` goes through Launch Services, which
starts the process in the GUI login session where it registers, activates
and receives keyboard input normally. `scripts/bundle-macos.sh` only
copies the binary and writes a minimal `Info.plist`; nothing in the app
depends on the bundle layout.

Logs go to the terminal on Linux/Windows. When launched through `open`
they go to the system log instead; to see them, run the bundled binary
directly from a **non-tmux** terminal:

```sh
target/release/ModPlayer.app/Contents/MacOS/modplayer
```

### 4. First launch

1. **Welcome / privacy notice** — read and acknowledge.
2. **Sign in** — the app opens Spotify's authorisation page in your
   browser (PKCE flow); approve and return to ModPlayer.
3. **Device check** — pick an output device and confirm you hear the test
   tone.
4. ModPlayer registers itself as a Spotify Connect device. Search or
   browse your library in the app, or pick **ModPlayer** as the playback
   device from any Spotify client.

Settings and account state are stored in the platform config directory
(macOS: `~/Library/Application Support/ModPlayer/`); the library mirror and
play log live alongside. Set `MODPLAYER_CONFIG_DIR=/some/dir` to point a
run at a different (e.g. throwaway) configuration.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo deny check
scripts/check-license-headers.sh
```

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs the same
five steps on Linux, macOS and Windows. Every `.rs` file starts with
`// SPDX-License-Identifier: MIT OR Apache-2.0`.

### Workspace layout

| Crate | Role |
|---|---|
| `modplayer` | Binary entrypoint: wires everything together, runs the `eframe` loop |
| `modplayer-ui` | `egui` screens, shell, settings, waveform, plugin UI surfaces |
| `modplayer-core` | Playback controller, settings, library, actions, plugin host |
| `modplayer-engine` | Real-time audio graph |
| `modplayer-effects` | Built-in DSP nodes (transpose, time-stretch, …) |
| `modplayer-audio-io` | `cpal` output backend |
| `modplayer-audio-source` | Source trait shared by receivers |
| `modplayer-audio-source-connect` | Spotify Connect receiver (`librespot`) |
| `modplayer-audio-source-synthetic` | Test-tone / fixture source |
| `modplayer-account` | Sign-in, token refresh, tier check |
| `modplayer-secure-store` | OS keyring wrapper |
| `modplayer-capability-gateway` | Permission gate between plugins and host services |
| `modplayer-plugin-runtime` | Luau VM, one thread per plugin |

### Plugins

Plugins are Luau scripts declared by a `plugin.toml` manifest and run in a
sandboxed VM with explicit, user-approved permissions. The API is
documented in [`docs/plugin-api/v1.md`](docs/plugin-api/v1.md). The two
bundled plugins under [`plugins/bundled/`](plugins/bundled/) — **Section
Loop** and **Key & Tempo** — are compiled into the host and serve as
reference implementations.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at
your option.
