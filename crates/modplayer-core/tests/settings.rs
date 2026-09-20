// SPDX-License-Identifier: MIT OR Apache-2.0

//! Settings-file contract tests (FR-020, FR-025e; contracts/settings-file.md):
//! round-trip, out-of-range clamping, a garbage file, a simulated crash
//! mid-write, an unknown key, and a newer schema version. Exercises mostly
//! `SettingsStore`'s public API, each test against its own directory under
//! `MODPLAYER_CONFIG_DIR`-style isolation via `SettingsStore::with_path` (no
//! test touches the real platform config location). T077 (007, US4)
//! additionally confirms one `[keybindings]` load path through the real
//! `PlaybackController::new` — the warning must reach the app's
//! `NotificationCenter`, not only `LoadOutcome`, which the tests above only
//! check in isolation; `crates/modplayer-core/tests/controller_actions.rs`'s
//! `custom_binding_survives_controller_restart` (T076) covers the matching
//! round-trip-through-controller half.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use modplayer_audio_io::FakeBackend;
use modplayer_audio_source_synthetic::SyntheticHost;
use modplayer_core::settings::{
    AudioSettings, DeviceName, DisclosureAcknowledgement, InvalidField, PanelPersisted,
    PanelPlacement, RawPanel, RawSettings, SettingsStore, SettingsWarning,
    generate_connect_device_id,
};
use modplayer_core::{Chord, FocusPolicy, HostAction, PlaybackController};
use modplayer_engine::{BufferPreset, VolumePercent};
use proptest::prelude::*;

/// Parse a literal that must be valid grammar (test convenience).
fn chord(s: &str) -> Chord {
    Chord::parse(s).unwrap_or_else(|_| unreachable!("{s:?} must be valid grammar"))
}

/// A minimal self-cleaning temp directory (no `tempfile` dependency).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "modplayer-settings-integration-{}-{}",
            std::process::id(),
            unique
        ));
        let _ = fs::create_dir_all(&dir);
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn store_in(dir: &TempDir) -> SettingsStore {
    SettingsStore::with_path(dir.path().join("settings.toml"))
}

#[test]
fn round_trip_defaults() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let settings = AudioSettings::default();
    assert!(store.save(&settings).is_ok());

    let outcome = store.load();
    assert_eq!(outcome.settings, settings);
    assert!(outcome.warnings.is_empty());
}

#[test]
fn disclosure_acknowledgement_round_trips_through_the_file() {
    // 002-first-launch-and-sign-in DM-27: the `[disclosure]` section must
    // survive a real save/load through `settings.toml` on disk, not just
    // the in-memory `RawSettings` conversion (`settings/model.rs`'s own
    // unit tests already cover that half).
    let dir = TempDir::new();
    let store = store_in(&dir);
    let settings = AudioSettings {
        disclosure: Some(DisclosureAcknowledgement {
            acknowledged_version: 1,
            acknowledged_at: time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(3600),
        }),
        ..AudioSettings::default()
    };
    assert!(store.save(&settings).is_ok());

    let outcome = store.load();
    assert_eq!(outcome.settings, settings);
    assert!(outcome.warnings.is_empty());

    let on_disk = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(
        on_disk.contains("[disclosure]") && on_disk.contains("acknowledged_version = 1"),
        "settings.toml must persist the [disclosure] section, got:\n{on_disk}"
    );
}

#[test]
fn out_of_range_file_values_clamp() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = "schema_version = 1\n\n[audio]\nlimiter_ceiling_db = 3.0\nmaster_volume = 250\n\n[audio.safe_volume]\ncap = -5\n";
    let _ = fs::write(store.path(), content);

    let outcome = store.load();
    assert!(
        outcome.warnings.is_empty(),
        "out-of-range numbers clamp silently, no notification"
    );
    assert_eq!(outcome.settings.limiter_ceiling_db.db(), -0.1);
    assert_eq!(outcome.settings.master_volume.value(), 100);
    assert_eq!(outcome.settings.safe_volume.cap, VolumePercent::new(0));
}

#[test]
fn garbage_file_loads_defaults_with_exactly_one_warning() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let _ = fs::write(store.path(), "this is { not valid toml at all");

    let outcome = store.load();
    assert_eq!(outcome.settings, AudioSettings::default());
    assert_eq!(outcome.warnings, vec![SettingsWarning::Unreadable]);
}

#[test]
fn simulated_crash_mid_write_leaves_prior_file_intact() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let original = AudioSettings {
        master_volume: VolumePercent::new(42),
        ..AudioSettings::default()
    };
    assert!(store.save(&original).is_ok());

    // Simulate a crash between writing `.tmp` and renaming it: leave a
    // half-written tmp file behind without ever calling `rename`.
    let tmp_path = store.path().with_file_name("settings.toml.tmp");
    let _ = fs::write(&tmp_path, "garbage that must never become the real file");

    let outcome = store.load();
    assert_eq!(
        outcome.settings, original,
        "prior file must survive an interrupted write"
    );
    assert!(outcome.warnings.is_empty());
}

#[test]
fn unknown_key_and_newer_schema_version_behave_as_contracted() {
    let dir = TempDir::new();

    // Unknown key: ignored, no warning, known fields still apply.
    let store = store_in(&dir);
    let content = "schema_version = 1\nfuture_top_level_key = 123\n\n[audio]\nbuffer_preset = \"performance\"\n";
    let _ = fs::write(store.path(), content);
    let outcome = store.load();
    assert!(outcome.warnings.is_empty());
    assert_eq!(outcome.settings.buffer_preset, BufferPreset::Performance);

    // Newer schema version: defaults + warning, and the file is not rewritten by load().
    let newer_dir = TempDir::new();
    let newer_store = store_in(&newer_dir);
    let _ = fs::write(newer_store.path(), "schema_version = 99\n");
    let before = fs::read_to_string(newer_store.path()).unwrap_or_default();

    let newer_outcome = newer_store.load();
    assert_eq!(newer_outcome.settings, AudioSettings::default());
    assert_eq!(newer_outcome.warnings, vec![SettingsWarning::NewerVersion]);

    let after = fs::read_to_string(newer_store.path()).unwrap_or_default();
    assert_eq!(before, after, "load() must not rewrite a newer-schema file");
}

#[test]
fn playback_section_round_trips_through_the_file() {
    // contracts/transport-and-queue.md §5: `[playback] device_name` /
    // `connect_device_id` must survive a real save/load through
    // `settings.toml`, not just the in-memory `RawSettings` conversion.
    let dir = TempDir::new();
    let store = store_in(&dir);
    let settings = AudioSettings {
        device_name: DeviceName::parse("Studio Mac").unwrap_or_default(),
        connect_device_id: Some("0123456789abcdef0123456789abcdef".to_string()),
        ..AudioSettings::default()
    };
    assert!(store.save(&settings).is_ok());

    let outcome = store.load();
    assert_eq!(outcome.settings, settings);
    assert!(outcome.warnings.is_empty());

    let on_disk = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(
        on_disk.contains("[playback]") && on_disk.contains("Studio Mac"),
        "settings.toml must persist the [playback] section, got:\n{on_disk}"
    );
}

#[test]
fn absent_playback_section_means_default_device_name() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let _ = fs::write(store.path(), "schema_version = 1\n");
    let outcome = store.load();
    assert!(outcome.warnings.is_empty());
    assert_eq!(outcome.settings.device_name, None);
    assert_eq!(outcome.settings.connect_device_id, None);
}

#[test]
fn device_name_over_64_chars_falls_back_to_default_with_a_warning() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let too_long = "x".repeat(65);
    let content = format!("schema_version = 1\n\n[playback]\ndevice_name = \"{too_long}\"\n");
    let _ = fs::write(store.path(), content);

    let outcome = store.load();
    assert_eq!(outcome.settings.device_name, None);
    assert_eq!(
        outcome.warnings,
        vec![SettingsWarning::InvalidValue(vec![
            InvalidField::DeviceName
        ])]
    );
}

#[test]
fn empty_device_name_means_default_with_no_warning() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = "schema_version = 1\n\n[playback]\ndevice_name = \"   \"\n";
    let _ = fs::write(store.path(), content);

    let outcome = store.load();
    assert_eq!(outcome.settings.device_name, None);
    assert!(outcome.warnings.is_empty());
}

#[test]
fn malformed_connect_device_id_is_dropped_silently() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = "schema_version = 1\n\n[playback]\nconnect_device_id = \"not-hex!\"\n";
    let _ = fs::write(store.path(), content);

    let outcome = store.load();
    assert_eq!(outcome.settings.connect_device_id, None);
    assert!(outcome.warnings.is_empty());
}

#[test]
fn device_name_validation() {
    assert_eq!(DeviceName::parse("").unwrap_or(None), None);
    assert_eq!(DeviceName::parse("   ").unwrap_or(None), None);
    assert_eq!(
        DeviceName::parse("  Studio Mac  ")
            .unwrap_or(None)
            .map(|d| d.as_str().to_string()),
        Some("Studio Mac".to_string())
    );
    assert!(DeviceName::parse(&"x".repeat(64)).is_ok());
    assert!(DeviceName::parse(&"x".repeat(65)).is_err());
}

#[test]
fn nudge_step_setting_round_trips_and_clamps() {
    // Round-trips through a real save/load (006, data-model.md §5).
    let dir = TempDir::new();
    let store = store_in(&dir);
    let settings = AudioSettings {
        nudge_step_ms: 25,
        ..AudioSettings::default()
    };
    assert!(store.save(&settings).is_ok());
    let outcome = store.load();
    assert_eq!(outcome.settings.nudge_step_ms, 25);
    assert!(outcome.warnings.is_empty());

    // Out-of-range values on disk clamp silently, no warning.
    let too_low = "schema_version = 1\n\n[markers]\nnudge_step_ms = 0\n";
    let _ = fs::write(store.path(), too_low);
    let outcome = store.load();
    assert_eq!(outcome.settings.nudge_step_ms, 1);
    assert!(outcome.warnings.is_empty());

    let too_high = "schema_version = 1\n\n[markers]\nnudge_step_ms = 5000\n";
    let _ = fs::write(store.path(), too_high);
    let outcome = store.load();
    assert_eq!(outcome.settings.nudge_step_ms, 1_000);
    assert!(outcome.warnings.is_empty());

    // Missing `[markers]` section means the default (10 ms).
    let _ = fs::write(store.path(), "schema_version = 1\n");
    let outcome = store.load();
    assert_eq!(outcome.settings.nudge_step_ms, 10);
    assert!(outcome.warnings.is_empty());
}

#[test]
fn generate_connect_device_id_is_32_hex_chars() {
    let id = generate_connect_device_id();
    assert_eq!(id.len(), 32);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
}

// ---------------------------------------------------------------------
// 007: `[keybindings]` (contracts/keymap-settings.md)
// ---------------------------------------------------------------------

#[test]
fn keybindings_table_absent_loads_defaults_silently() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let _ = fs::write(store.path(), "schema_version = 1\n");

    let outcome = store.load();

    assert!(outcome.warnings.is_empty());
    assert!(outcome.settings.keybinding_overrides.is_empty());
}

#[test]
fn keybindings_round_trip_is_sparse() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let mut settings = AudioSettings::default();
    settings
        .keybinding_overrides
        .set(HostAction::ToggleLoop, vec![chord("K")]);
    assert!(store.save(&settings).is_ok());

    let outcome = store.load();
    assert!(outcome.warnings.is_empty());
    assert_eq!(
        outcome
            .settings
            .keybinding_overrides
            .get(HostAction::ToggleLoop),
        Some([chord("K")].as_slice())
    );

    let on_disk = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(
        on_disk.contains("[keybindings]") && on_disk.contains("host.loop.toggle"),
        "only the one customized action must appear, got:\n{on_disk}"
    );

    // Reset it: the table disappears entirely (sparse by construction).
    let mut reset_settings = outcome.settings;
    reset_settings
        .keybinding_overrides
        .remove(HostAction::ToggleLoop);
    assert!(store.save(&reset_settings).is_ok());
    let on_disk_after_reset = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(
        !on_disk_after_reset.contains("[keybindings]"),
        "a fully-reset map must write no [keybindings] table, got:\n{on_disk_after_reset}"
    );
}

#[test]
fn keybindings_invalid_entries_dropped_in_isolation() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = r#"schema_version = 1

[keybindings]
"host.loop.toggle" = ["K"]
"host.nope.x" = ["A"]
"host.markers.set_a" = ["Not+A+Valid+Chord"]
"host.markers.set_b" = "not-an-array"
"#;
    let _ = fs::write(store.path(), content);

    let outcome = store.load();

    assert_eq!(
        outcome
            .settings
            .keybinding_overrides
            .get(HostAction::ToggleLoop),
        Some([chord("K")].as_slice()),
        "the one good entry must still apply"
    );
    match &outcome.warnings[..] {
        [SettingsWarning::InvalidKeybindings(ids)] => {
            let mut ids = ids.clone();
            ids.sort();
            assert_eq!(
                ids,
                vec!["host.markers.set_a", "host.markers.set_b", "host.nope.x"]
            );
        }
        other => panic!("expected exactly one InvalidKeybindings warning, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T063 (US2, 011-plugin-ui-contributions, contracts/action-registry-
// plugins.md K1/K2, FR-010a)
// ---------------------------------------------------------------------

/// A `[keybindings]` id outside the `host.` namespace, with a decodable
/// value, is retained dormant — never dropped, never warned about — and
/// re-serialised verbatim on the next save (K1).
#[test]
fn non_host_keybinding_retained_dormant() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = r#"schema_version = 1

[keybindings]
"host.loop.toggle" = ["K"]
"org.modplayer.fixture.ui-shortcuts.take_over" = ["Shift+K"]
"#;
    let _ = fs::write(store.path(), content);

    let outcome = store.load();
    assert!(
        outcome.warnings.is_empty(),
        "a non-host id must never raise a warning: {:?}",
        outcome.warnings
    );

    let dormant: Vec<(String, Vec<Chord>)> = outcome
        .settings
        .keybinding_overrides
        .dormant_iter()
        .map(|(id, chords)| (id.clone(), chords.to_vec()))
        .collect();
    assert_eq!(
        dormant,
        vec![(
            "org.modplayer.fixture.ui-shortcuts.take_over".to_string(),
            vec![chord("Shift+K")]
        )]
    );

    // Round-trips verbatim: still there, unchanged, after a save/reload.
    assert!(store.save(&outcome.settings).is_ok());
    let reloaded = store.load();
    assert!(reloaded.warnings.is_empty());
    assert_eq!(
        reloaded
            .settings
            .keybinding_overrides
            .dormant_iter()
            .map(|(id, chords)| (id.clone(), chords.to_vec()))
            .collect::<Vec<_>>(),
        dormant
    );
    let on_disk = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(
        on_disk.contains("org.modplayer.fixture.ui-shortcuts.take_over"),
        "the dormant entry must be re-serialised verbatim, got:\n{on_disk}"
    );
}

/// K2 unchanged: only a `host.`-namespaced unknown id (or one whose
/// value fails to decode) is dropped and warned about — the dormant path
/// above (T063's other half) never raises this warning.
#[test]
fn host_unknown_still_warned() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = r#"schema_version = 1

[keybindings]
"host.nope.x" = ["A"]
"org.modplayer.fixture.ui-shortcuts.take_over" = ["Shift+K"]
"#;
    let _ = fs::write(store.path(), content);

    let outcome = store.load();
    match &outcome.warnings[..] {
        [SettingsWarning::InvalidKeybindings(ids)] => {
            assert_eq!(ids, &vec!["host.nope.x".to_string()]);
        }
        other => panic!("expected exactly one InvalidKeybindings warning, got {other:?}"),
    }
    assert_eq!(
        outcome.settings.keybinding_overrides.dormant_iter().count(),
        1,
        "the non-host id must still land in dormant, unaffected by the host warning"
    );
}

#[test]
fn keybindings_bad_entries_rewritten_clean_on_next_save() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = "schema_version = 1\n\n[keybindings]\n\"host.loop.toggle\" = [\"K\"]\n\"host.nope.x\" = [\"A\"]\n";
    let _ = fs::write(store.path(), content);
    let outcome = store.load();

    assert!(store.save(&outcome.settings).is_ok());

    let on_disk = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(on_disk.contains("host.loop.toggle"));
    assert!(
        !on_disk.contains("host.nope.x"),
        "the dropped entry must not survive a re-save, got:\n{on_disk}"
    );
}

#[test]
fn whole_file_garbage_still_single_unreadable_warning() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let _ = fs::write(store.path(), "not valid toml at all {{{");

    let outcome = store.load();

    assert_eq!(
        outcome.warnings,
        vec![SettingsWarning::Unreadable],
        "an unreadable file must never also raise an InvalidKeybindings warning"
    );
}

#[test]
fn crash_mid_write_keeps_previous_keybindings() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let mut original = AudioSettings::default();
    original
        .keybinding_overrides
        .set(HostAction::ToggleLoop, vec![chord("K")]);
    assert!(store.save(&original).is_ok());

    let tmp_path = store.path().with_file_name("settings.toml.tmp");
    let _ = fs::write(&tmp_path, "garbage that must never become the real file");

    let outcome = store.load();

    assert_eq!(outcome.settings, original);
    assert!(outcome.warnings.is_empty());
}

// ---------------------------------------------------------------------
// T077 (US4): the `[keybindings]` load path through the real
// `PlaybackController`, not only `SettingsStore`/`KeymapOverrides` in
// isolation (T024/T027 above and in `tests/actions.rs`).
// ---------------------------------------------------------------------

#[test]
fn keybindings_invalid_entries_warning_reaches_controller_notifications() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = "schema_version = 1\n\n[keybindings]\n\"host.loop.toggle\" = [\"K\"]\n\"host.nope.x\" = [\"A\"]\n";
    let _ = fs::write(store.path(), content);

    let controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    // The good entry reaches the running controller's registry, not just
    // `LoadOutcome`.
    assert_eq!(
        controller.actions().bindings(HostAction::ToggleLoop),
        &[chord("K")],
        "the one good entry must still apply through the controller"
    );

    // The warning `PlaybackController::new` raises each `LoadOutcome`
    // warning as (contracts/keymap-settings.md) actually reaches the
    // controller's `NotificationCenter`, naming the dropped id.
    let notification = controller
        .notifications()
        .all()
        .find(|n| n.message_key == "keybindings-invalid-entries")
        .unwrap_or_else(|| {
            panic!("PlaybackController::new must raise keybindings-invalid-entries")
        });
    assert_eq!(notification.args, vec![("ids", "host.nope.x".to_string())]);
}

// ---------------------------------------------------------------------
// 010-transport-focus (US2-8, SC-007, FR-012): `[transport] focus_policy`
// (data-model.md §2.2) — the holder and pending queue are session-only
// and never appear here at all (`AudioSettings` itself has no field for
// either), so a round-trip of `AudioSettings` is already, by
// construction, a round-trip of the policy alone.
// ---------------------------------------------------------------------

#[test]
fn settings_round_trip_focus_policy() {
    let dir = TempDir::new();
    let store = store_in(&dir);

    for policy in FocusPolicy::ALL {
        let settings = AudioSettings {
            focus_policy: policy,
            ..AudioSettings::default()
        };
        assert!(store.save(&settings).is_ok());

        let outcome = store.load();
        assert_eq!(outcome.settings.focus_policy, policy);
        assert!(outcome.warnings.is_empty());
    }

    let on_disk = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(
        on_disk.contains("[transport]") && on_disk.contains("focus_policy"),
        "settings.toml must persist the [transport] section, got:\n{on_disk}"
    );
}

#[test]
fn absent_transport_section_means_default_focus_policy_no_warning() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let _ = fs::write(store.path(), "schema_version = 1\n");

    let outcome = store.load();
    assert_eq!(
        outcome.settings.focus_policy,
        FocusPolicy::AutoOnInteraction
    );
    assert!(outcome.warnings.is_empty());
}

#[test]
fn unknown_focus_policy_falls_back_to_default_with_a_warning() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let content = "schema_version = 1\n\n[transport]\nfocus_policy = \"not_a_policy\"\n";
    let _ = fs::write(store.path(), content);

    let outcome = store.load();
    assert_eq!(
        outcome.settings.focus_policy,
        FocusPolicy::AutoOnInteraction
    );
    assert_eq!(
        outcome.warnings,
        vec![SettingsWarning::InvalidValue(vec![
            InvalidField::FocusPolicy
        ])]
    );
}

// Constitution VIII: a small state-serialization proptest extension —
// every one of `FocusPolicy::ALL`'s three variants round-trips through a
// real save/load, exactly like `settings_round_trip_focus_policy` above
// proves in a plain loop, but here driven by an arbitrary `proptest`
// index selection strategy rather than a fixed iteration order.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn focus_policy_round_trips_for_any_arbitrary_variant(idx in 0usize..FocusPolicy::ALL.len()) {
        let policy = FocusPolicy::ALL[idx];
        let dir = TempDir::new();
        let store = store_in(&dir);
        let settings = AudioSettings {
            focus_policy: policy,
            ..AudioSettings::default()
        };
        prop_assert!(store.save(&settings).is_ok());

        let outcome = store.load();
        prop_assert_eq!(outcome.settings.focus_policy, policy);
        prop_assert!(outcome.warnings.is_empty());
    }

    // 011-plugin-ui-contributions (US1 T040, contracts/ui-panels.md L3):
    // an arbitrary `[plugin_panels]` entry — any placement, any (finite)
    // geometry, disabled or not — round-trips through a real save/load
    // with no warning.
    #[test]
    fn plugin_panels_round_trip_proptest(
        floated in any::<bool>(),
        x in -1_000.0f32..3_000.0,
        y in -1_000.0f32..3_000.0,
        w in 50.0f32..2_000.0,
        h in 50.0f32..2_000.0,
        disabled in any::<bool>(),
    ) {
        let dir = TempDir::new();
        let store = store_in(&dir);
        let mut settings = AudioSettings::default();
        settings.plugin_panels.insert(
            "org.modplayer.fixture.ui-panel/main".to_string(),
            PanelPersisted {
                placement: if floated { PanelPlacement::Floated } else { PanelPlacement::Docked },
                x: Some(x),
                y: Some(y),
                w: Some(w),
                h: Some(h),
                disabled,
            },
        );
        prop_assert!(store.save(&settings).is_ok());

        let outcome = store.load();
        prop_assert_eq!(outcome.settings.plugin_panels, settings.plugin_panels);
        prop_assert!(outcome.warnings.is_empty());
    }
}

/// 011-plugin-ui-contributions (US1 T040, contracts/ui-panels.md L3): an
/// unrecognised `placement` string drops only that `[plugin_panels]`
/// entry, with a warning — never the whole table, and never a load
/// failure.
#[test]
fn plugin_panels_bad_placement_dropped() {
    let mut raw = RawSettings::default();
    raw.plugin_panels.insert(
        "org.modplayer.fixture.ui-panel/main".to_string(),
        RawPanel {
            placement: "sideways".to_string(),
            x: None,
            y: None,
            w: None,
            h: None,
            disabled: false,
        },
    );
    let (settings, invalid, dropped) = raw.into_settings();
    assert!(dropped.is_empty());
    assert!(settings.plugin_panels.is_empty());
    assert_eq!(
        invalid,
        vec![InvalidField::PluginPanel(
            "org.modplayer.fixture.ui-panel/main".to_string()
        )]
    );
}

// ---------------------------------------------------------------------
// 013-key-and-tempo-plugin (US4, contracts/getting-started-card.md S1/S2):
// `[onboarding] getting_started_dismissed` — device-scoped, absent ⇒
// `false`, untouched by sign-out, same file-section handling as
// `[disclosure]` above.
// ---------------------------------------------------------------------

#[test]
fn getting_started_flag_round_trip() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let settings = AudioSettings {
        getting_started_dismissed: true,
        ..AudioSettings::default()
    };
    assert!(store.save(&settings).is_ok());

    let outcome = store.load();
    assert_eq!(outcome.settings, settings);
    assert!(outcome.warnings.is_empty());

    let on_disk = fs::read_to_string(store.path()).unwrap_or_default();
    assert!(
        on_disk.contains("[onboarding]") && on_disk.contains("getting_started_dismissed = true"),
        "settings.toml must persist the [onboarding] section, got:\n{on_disk}"
    );
}

#[test]
fn getting_started_flag_absent_section_means_not_dismissed() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let _ = fs::write(store.path(), "schema_version = 1\n");

    let outcome = store.load();
    assert!(!outcome.settings.getting_started_dismissed);
    assert!(outcome.warnings.is_empty());
}

#[test]
fn getting_started_flag_survives_sign_out() {
    let dir = TempDir::new();
    let store = store_in(&dir);
    let mut controller =
        PlaybackController::new(FakeBackend::new(vec![]), SyntheticHost::new(44_100), store);

    controller.dismiss_getting_started();
    assert!(controller.getting_started_dismissed());

    // `clear_for_sign_out` (002/003) must never reset the device-scoped
    // onboarding flag — it is not account data (S2).
    controller.clear_for_sign_out();
    assert!(
        controller.getting_started_dismissed(),
        "sign-out must leave [onboarding] intact"
    );

    let on_disk = fs::read_to_string(controller.settings_store().path()).unwrap_or_default();
    assert!(
        on_disk.contains("[onboarding]") && on_disk.contains("getting_started_dismissed = true"),
        "the persisted flag must also survive sign-out, got:\n{on_disk}"
    );
}
