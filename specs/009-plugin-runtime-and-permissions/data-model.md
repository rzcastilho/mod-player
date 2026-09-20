# Data Model: Plugin Runtime, Sandbox, and Permission Gateway

**Feature**: 009-plugin-runtime-and-permissions | **Date**: 2026-09-19
**Inputs**: [spec.md](spec.md) Key Entities, [research.md](research.md) R2–R17

Types are grouped by owning crate. Existing types are marked `~` (modified)
and new ones `+`. Rust names are the ones tasks and contracts use.

---

## 1. `modplayer-capability-gateway` (new crate)

### 1.1 Generated from `api/v1.toml` (research R6)

```text
+ enum Permission            // 25 catalog entries, e.g. PlaybackObserve, TransportControl,
                             // QueueWrite, MarkersRead, MarkersWrite, AudioEffects,
                             // AudioMeter, StatePlugin, StateTrack, Network, AudioProcess,
                             // UiPanel, UiOverlay, UiShortcut, UiSettings, FilesRead,
                             // FilesWrite, MidiIn, MidiOut, Clipboard, LibraryRead,
                             // LibraryWrite, AnalysisRead, AnalysisRequest, Notifications
    fn name(self) -> &'static str          // "playback.observe"
    fn explanation_key(self) -> &'static str // Fluent key "permission-playback-observe"
    fn operable(self) -> bool               // true for the 9 this slice implements
    fn parse(&str) -> Option<Permission>

+ enum RequestKind           // one per request, e.g. TransportSeek, MarkersCreate …
    fn requires(self) -> Option<Permission>  // None only for timers/debug_probe
    fn needs_focus(self) -> bool
    fn category(self) -> RateCategory
    fn lua_path(self) -> (&'static str, &'static str) // ("transport", "seek")
    const ALL: &'static [RequestKind]

+ enum EventKind             // TrackChanged, Position, PlayStateChanged, QueueChanged,
                             // MarkerChanged, LoopArmed, LoopDisarmed, LoopWrapped,
                             // EffectChainChanged, Meter, ReadyAck, Unloading, Timer,
                             // PositionReached
    fn requires(self) -> Option<Permission>
    fn name(self) -> &'static str

+ const API_VERSION: ApiVersion = ApiVersion { major: 1, minor: 0 }
+ const HOST_CAPABILITIES: &[&str]  // the 9 operable permission names + "timers"
```

### 1.2 Manifest (FR-001, FR-002; DM-10 "validated manifest")

```text
+ struct Manifest {
    identifier: PluginIdentifier,      // reverse-domain, immutable
    name: String, description: String,
    version: Version,                  // { major, minor, patch }
    api: ApiRange,                     // { major: u16, min_minor: u16 } ⇒ ≥ major.min_minor, < major+1
    author: String, license: String, homepage: Option<String>, source: String,
    entry: String,                     // default "main.luau"
    required: Vec<PermissionRequest>,  // { permission: Permission, justification: String }
    optional: Vec<PermissionRequest>,
    network_hosts: Vec<String>,        // recorded only
    ui: Vec<UiContribution>,           // recorded only: { kind: String, id: String, title: String }
    effect_nodes: Vec<IntendedNode>,   // { kind: String, suggested_position: Option<SuggestedPosition> }
    min_host_version: Option<Version>,
  }
+ struct PluginIdentifier(String)    // ≥ 2 dot-separated labels of [a-z0-9-], ≤ 128 bytes
+ struct Version { major: u32, minor: u32, patch: u32 }   // Ord
+ enum SuggestedPosition { Index(usize), Before(String), After(String) }
+ enum ManifestError {              // every variant renders to one Fluent sentence
    MissingField(&'static str), MalformedField { field: &'static str, detail: String },
    UnknownPermission { permission: String, list: PermissionList /* Required|Optional */ },
    NetworkMustBeOptional, NetworkWithoutHosts, DuplicatePermission(Permission),
    UnreadableManifest(String), EntryMissing(String),
  }
```

Validation rules (in order; the first failure is the reason shown):
1. `plugin.toml` parses as TOML → else `UnreadableManifest`.
2. Required fields present: `identifier`, `name`, `version`, `api`,
   `author`, `license`, `source`, `entry` (defaulted) → `MissingField`.
3. Each field well-formed (identifier grammar, version triple, api range,
   non-empty name/author) → `MalformedField`.
4. Every permission string in `required` and `optional` parses as one of
   the 25 catalog entries → `UnknownPermission` ("invalid manifest: …").
5. `network` not in `required` → `NetworkMustBeOptional`; `network` in
   `optional` requires a non-empty `network_hosts` → `NetworkWithoutHosts`.
6. No permission appears twice across both lists → `DuplicatePermission`.
7. The entry file exists in the package → `EntryMissing`.

### 1.3 Grants (DM-11 PermissionGrant)

```text
+ enum GrantState { Granted, Denied, NotRequested }
+ struct PermissionGrant { permission: Permission, state: GrantState,
                           granted_at: Option<SystemTime>, granted_by: GrantedBy /* Install */,
                           hosts: Vec<String> /* network only, unused */ }
+ struct Grants([GrantState; 25])   // indexed by Permission; Copy; built from a Manifest
    fn from_bundled(&Manifest) -> Grants     // required ∪ optional ⇒ Granted (FR-003 pre-approved)
    fn holds(&self, Permission) -> bool
    fn granted(&self) -> impl Iterator<Item = Permission>
    fn rows(&self) -> Vec<PermissionGrant>   // for the list / DM-11 view
```

### 1.4 Refusal, requests, responses (FR-008)

```text
+ enum RefusalCode { PermissionDenied, NoFocus, InvalidState, NotFound, RateLimited, BudgetExceeded }
+ struct Refusal { code: RefusalCode, reason: &'static str, message: String }
    // reason ∈ { "not_granted", "not_owner", "no_focus", "focus_held", "rate_limited",
    //            "unknown_id", "chain_full", "marker_limit", "timer_limit", "storage_cap",
    //            "key_too_long", "value_too_large", "region_incomplete", "region_too_short",
    //            "nothing_armed", "not_ready", "host_busy", "no_track", "unsupported_kind",
    //            "invalid_argument" }
+ type PluginResult<T> = Result<T, Refusal>

+ enum Request {                      // exactly the RequestKind set, with arguments
    // transport (needs_focus unless noted)
    Play, Pause, Toggle, Seek { position_ms: u64 }, SkipNext, SkipPrevious,
    RequestFocus /* no focus needed */, ReleaseFocus /* no focus needed */,
    ArmLoop { region: RegionId }, DisarmLoop,
    // queue.write (never focus-gated)
    QueueList, QueueMove { item: QueueItemId, to: usize }, QueueRemove { item: QueueItemId },
    QueuePlayNext { item: QueueItemId }, QueueAdd { track: String },
    // markers.read / markers.write
    ListMarkers, CreateMarker { position_ms: u64, name: Option<String>, transient: bool },
    MoveMarker { id: MarkerId, position_ms: u64 }, RenameMarker { id, name }, RecolorMarker { id, color: u8 },
    DeleteMarker { id }, CreateLoopRegion { a_ms: u64, b_ms: u64, transient: bool },
    SetCue { slot: u8, position_ms: u64 },
    // audio.effects
    ListChain, CreateNode { kind: String, suggested: Option<SuggestedPosition> },
    SetParam { node: NodeId, param: u8, value: f32 }, ScheduleParam { node, param, value, at_ms: u64 },
    SetBypass { node: NodeId, bypassed: bool }, RemoveNode { node: NodeId },
    // fixture-only
    DebugProbe { name: String },
  }
+ enum Response { Ok, MarkerId(MarkerId), RegionId(RegionId), NodeId(NodeId),
                  Markers(Vec<MarkerInfo>), Chain(Vec<NodeInfo>), Queue(Vec<QueueItemInfo>),
                  Probe(serde_json::Value) }
+ struct MarkerInfo { id, kind: String, position_ms, name, color, owner: OwnerInfo, transient, region: Option<RegionId>, slot: Option<u8> }
+ struct NodeInfo { id, kind: String, owner: OwnerInfo, bypassed, auto_bypassed, orphaned, index }
+ struct QueueItemInfo { id, track: String, title: String, index, is_current }
+ enum OwnerInfo { Host, Plugin(String /* identifier */), Me }
```

Ids cross the boundary as `u32`/`u64` newtypes mirrored from core
(`MarkerId`, `RegionId`, `NodeId`, `QueueItemId` are re-declared as
transparent `u32` wrappers here; core converts).

### 1.5 Host → plugin events (FR-016, FR-018, FR-019, FR-020, FR-028)

```text
+ enum HostEvent {
    ReadyAck { api: ApiVersion, granted: Vec<Permission>, capabilities: Vec<&'static str> },
    Unloading { reason: UnloadReason /* Disable | Suspend | Shutdown */ },
    TrackChanged { track: Option<TrackInfo> },       // per-track state restored before delivery
    Position { position_ms: u64 },
    PlayStateChanged { state: PlayState /* Playing | Paused | Stopped */ },
    QueueChanged { items: Vec<QueueItemInfo> },
    MarkerChanged { actor: OwnerInfo, revision: u64 },
    LoopArmed { region: RegionId, by: OwnerInfo }, LoopDisarmed { by: OwnerInfo }, LoopWrapped { region: RegionId, wraps: u32 },
    EffectChainChanged { chain: Vec<NodeInfo> },
    Meter { pre: LevelInfo, post: LevelInfo, spectrum: [f32; 64] },
    Timer { handle: TimerHandle },                    // set_timeout / set_interval fire
    PositionReached { handle: TimerHandle, position_ms: u64 },
  }
+ struct TrackInfo { id: String, title: String, artists: Vec<String>, duration_ms: u64 }
```

### 1.6 Gateway admission and rate limiting (FR-007, FR-025)

```text
+ enum RateCategory { Transport, Markers, Effects, State, Timers }
+ struct RateLimiter { windows: [VecDeque<Instant>; 5] }    // each cap 100
    fn admit(&mut self, RateCategory, now: Instant) -> Result<(), Refusal>
+ struct FocusToken(Arc<AtomicU16>)                          // 0 = host, n = PluginId(n-1)
    fn holder(&self) -> Option<PluginId>; fn try_acquire(&self, PluginId) -> bool; fn release_if(&self, PluginId)
+ struct Gateway { plugin: PluginId, grants: Grants, focus: FocusToken, limiter: RateLimiter }
    fn admit(&mut self, kind: RequestKind, now: Instant) -> Result<(), Refusal>
        // order: permission → focus → rate limit (spec FR-008); refused calls consume no quota
```

### 1.7 Plugin state store (DM-12, FR-014, FR-021)

```text
+ struct PluginStateStore {
    plugin: BTreeMap<String, serde_json::Value>,   // device-scoped
    track: Option<TrackScope>,                     // { track_id: String, entries: BTreeMap<…> }
    used_bytes: usize,                             // serialized size of every key+value, both scopes
    dirty: DirtyFlags,                             // { plugin: bool, track: bool }
  }
  const MAX_KEY_BYTES = 256; const MAX_VALUE_BYTES = 1 MiB; const STORAGE_CAP = 10 MiB
    fn get(&self, Scope, key) -> Option<&Value>
    fn set(&mut self, Scope, key, Value) -> Result<(), Refusal>   // budget_exceeded/{storage_cap,key_too_long,value_too_large}
    fn remove(&mut self, Scope, key)
    fn load_track(&mut self, track_id, bytes: &[u8]) -> Result<(), StoreError>
    fn encode(&self, Scope) -> Vec<u8>          // pretty JSON, sorted keys
+ struct PluginStatePaths { root: PathBuf }     // MODPLAYER_PLUGIN_STATE_DIR | data_local_dir/ModPlayer/plugin-state
    fn plugin_file(&self, id) -> PathBuf        // <root>/<hex(id)>/plugin.json
    fn track_file(&self, id, track) -> PathBuf  // <root>/<hex(id)>/tracks/<hex(track)>.json
    fn clear_tracks(&self, id)                  // sign-out
+ struct StateWriter (spawned thread; mpsc<WriteJob { path, bytes, ack: Option<SyncSender<()>> }>)
    // .tmp → write_all → sync_all → rename; 500 ms debounce per path; ack for the 200 ms unloading window
+ struct PluginStateEntry { scope: Scope, key: String, value: Value, updated_at: SystemTime } // DM-12 row view
```

Invariants: `used_bytes` equals the sum of `key.len() + serde_json::to_vec(value).len()`
over both scopes at all times; a failed `set` leaves every field unchanged.

### 1.8 Budget constants (FR-009, FR-006, FR-012)

```text
+ struct Budgets { handler: Duration /* 4 ms */, share: Duration /* 100 ms per 1 s */,
                   window: Duration /* 1 s */, memory: usize /* 64 MiB */, storage: usize /* 10 MiB */,
                   ready_timeout: Duration /* 5 s */, unload_window: Duration /* 200 ms */,
                   rpc_timeout: Duration /* 1 s */, max_timers: usize /* 256 */, inbox: usize /* 1024 */ }
  const DEFAULT: Budgets
```

## 2. `modplayer-plugin-runtime` (new crate)

```text
+ struct PluginContext { lua: mlua::Lua, budget: Arc<BudgetState>, gateway: Gateway,
                         store: PluginStateStore, timers: TimerSet, subscriptions: Subscriptions }
+ struct BudgetState { deadline_ns: AtomicU64, epoch: Instant, samples: Mutex<VecDeque<(Instant, Duration)>>,
                       gauges: Arc<PluginGauges> }
+ struct PluginGauges { cpu_permille_of_share: AtomicU16, used_bytes: AtomicU64, pending_timers: AtomicU16 } // read by the list
+ struct TimerSet { next_handle: u32, timeouts: BTreeMap<Instant, Vec<TimerHandle>>,
                    intervals: HashMap<TimerHandle, Duration>, at_position: Vec<(TimerHandle, u64 /* ms */)> }
    // ≤ 256 pending (FR-022); min interval 1 ms; position timers cleared on track generation change
+ struct TimerHandle(u32)
+ struct Subscriptions { position_rate_hz: u8 /* 1..=60, default 10 */, last_position_ms: Option<u64>,
                         meter: bool, last_meter_at: Option<Instant> }
+ enum Inbound { Event(HostEvent), Control(Control) }
+ enum Control { Unloading { reason: UnloadReason }, Stop, Probe(SyncSender<Value>) }
+ struct PluginHandle { id: PluginId, inbox: SyncSender<Inbound>, gauges: Arc<PluginGauges>,
                        thread: JoinHandle<()>, started_at: Instant }
+ enum RuntimeEvent {                       // plugin thread → core, drained in tick()
    Ready, HandlerAborted { cause: AbortCause /* Exception(String) | Deadline | RestoreTimeout */ },
    Suspended { cause: SuspendCause /* Hang | CpuShare | Memory | DidNotStart */ },
    Log { level: Level, message: String }, Exited,
  }
+ struct PlaybackSnapshot { intent: AtomicU8, source_rate: AtomicU32, track_generation: AtomicU64 } // core-written
+ struct RuntimeDeps { shared: Arc<RtShared>, playback: Arc<PlaybackSnapshot>,
                       requests: SyncSender<RpcEnvelope>, events: SyncSender<(PluginId, RuntimeEvent)>,
                       snapshot: Arc<Mutex<PluginSnapshot>>, writer: StateWriterHandle, paths: PluginStatePaths }
+ struct RpcEnvelope { plugin: PluginId, request: Request, reply: SyncSender<Response-or-Refusal> }
```

Scheduler state machine (per thread):

```text
Created ──entry script ok──▶ AwaitingReady ──ready()──▶ Active ──Unloading──▶ Draining ──▶ Exited
   │                              │                        │
   └─ script error ──▶ Exited     └─ 5 s ──▶ Exited        └─ share/memory breach ──▶ Exited
     (RuntimeEvent::Suspended{DidNotStart-or-Hang})        (RuntimeEvent::Suspended{cause}, after Unloading{Suspend})
```

## 3. `modplayer-core::plugins` (new module)

### 3.1 Plugin record (DM-10)

```text
+ struct PluginId(u16)                 // re-export of modplayer_effects::PluginId (session-stable index)
+ struct PluginIdTable { by_identifier: HashMap<PluginIdentifier, PluginId>, by_id: Vec<PluginIdentifier> } // interner
+ enum Source { Bundled }
+ enum Health { Ok, Warning, Suspended }          // DM-10 also models DisabledIncompatible: not constructed this slice
+ enum Lifecycle { Invalid(ManifestError), Disabled, Loading, Active, Suspended { cause: SuspendCause }, Draining }
+ struct PluginRecord {
    id: PluginId, identifier: PluginIdentifier, package: BundledPackage,
    manifest: Result<Manifest, ManifestError>,
    source: Source, enabled: bool, lifecycle: Lifecycle, health: Option<Health>,   // None while invalid
    grants: Grants, suspensions_this_session: u8, abort_window: VecDeque<Instant>, warning_until: Option<Instant>,
    handle: Option<PluginHandle>, gauges: Option<Arc<PluginGauges>>,
    api_range: ApiRange, budgets: Budgets, compatibility_mode: bool /* always false */,
  }
```

Lifecycle transitions (FR-005, FR-011, FR-012, FR-024):

```text
Invalid ─(never)─▶ …
Disabled ──enable()──▶ Loading ──Ready──▶ Active
Loading/Active ──Suspended{cause}──▶ Suspended   (suspensions += 1; if == 3 ⇒ disable())
Suspended ──Restart──▶ Loading           ──Disable──▶ Draining ──Exited──▶ Disabled
Active ──disable()──▶ Draining ──Exited──▶ Disabled
any running ──shutdown()──▶ Draining
```

Health derivation: `Suspended` while lifecycle is `Suspended`;
`Warning` while `warning_until > now`; else `Ok`; `None` for `Invalid`
and shown as "—" for CPU/memory when not `Active`.

### 3.2 Host state

```text
+ struct PluginHost {
    records: Vec<PluginRecord>,            // sorted by name for the view
    ids: PluginIdTable, focus: FocusToken, paths: Option<PluginStatePaths>, writer: StateWriterHandle,
    requests_rx: Receiver<RpcEnvelope>, requests_tx: SyncSender<RpcEnvelope>,
    events_rx: Receiver<(PluginId, RuntimeEvent)>, events_tx: …,
    playback: Arc<PlaybackSnapshot>, snapshot: Arc<Mutex<PluginSnapshot>>,
    log: PluginLog, fixtures_enabled: bool, waker: Option<Arc<dyn Fn() + Send + Sync>>,
  }
+ struct PluginSnapshot { markers: Vec<MarkerInfo>, chain: Vec<NodeInfo>, queue: Vec<QueueItemInfo>,
                          marker_revision: u64, chain_revision: u64 }
+ struct PluginLog { entries: VecDeque<LogEntry> /* cap 1000 */ }
+ struct LogEntry { at: Instant, plugin: PluginIdentifier, level: Level, message: String }
+ struct BundledPackage { identifier: &'static str, manifest_toml: &'static str, entry: &'static str, readme: &'static str, fixture: bool }
```

### 3.3 View (FR-023)

```text
+ struct PluginRow { id: PluginId, name: String, version: String, source: Source, enabled: bool,
                     health: Option<Health>, invalid_reason: Option<ManifestError>,
                     permissions: Vec<Permission> /* granted, catalog order */,
                     cpu_pct_of_share: Option<f32>, memory_bytes: Option<u64>, can_uninstall: bool /* always false */ }
+ struct PluginsView { rows: Vec<PluginRow> /* sorted by name */ }
```

### 3.4 Existing model deltas

```text
~ markers::model::Owner            { Host, + Plugin(PluginId) }          // still Copy
~ markers::model::Marker.transient // now set by CreateMarker/CreateLoopRegion { transient: true }
~ markers::model::MarkerError      { …, + NotOwner }
~ markers::model::TrackMarkers     { + revision: u64 (bumped on every mutation),
                                     + fn add_point_owned(pos, owner, transient), + fn new_loop_region_owned(a, b, owner, transient),
                                     + fn set_cue_owned(slot, pos, owner) /* not_owner if slot held by another owner */,
                                     + fn owner_of(MarkerId) -> Option<Owner>, + fn armed_region_owner() -> Option<Owner>,
                                     + fn remove_transient_owned_by(Owner) -> usize, + fn disarm_if_owned_by(Owner) -> bool }
~ markers::store DTO owner string  // "host" | "<identifier>"; encode() skips transient markers/regions
~ effects::model::ChainModel       { + revision: u64, + fn add_at(kind, owner, index), + fn orphan_owned_by(PluginId) -> Vec<NodeId>,
                                     + fn readopt(PluginId) -> Vec<NodeId>, + fn owned_by(PluginId), + fn resolve_position(&SuggestedPosition) -> usize }
~ effects::model::NodeModel.orphaned   // now set/cleared by the two functions above
~ notifications::Notification      { + dedupe_key: Option<String> }
~ notifications::NotificationAction { …, + RestartPlugin(PluginId), + DisablePlugin(PluginId) }
~ notifications::NotificationCenter { + fn raise_keyed(severity, message_key, args, actions, dedupe_key: String) -> u64,
                                      + fn dismiss_by_dedupe(&str) }
+ notifications::KEY_PLUGIN_SUSPENDED = "plugin-suspended"; KEY_PLUGIN_AUTO_DISABLED = "plugin-auto-disabled"
~ PlaybackController               { + plugins: PluginHost, + last_marker_actor: Owner, + fn set_waker(..),
                                     + fn plugins_view(), + fn plugin_enable(id), + fn plugin_disable(id), + fn plugin_restart(id),
                                     + fn plugin_log(), + fn plugins_mut() /* tests */ }
```

## 4. Fluent keys (new `locales/en-US/plugins.ftl`)

`plugins-title`, `plugins-empty`, `plugins-col-*` (8 columns),
`plugins-source-bundled`, `plugins-health-ok|warning|suspended`,
`plugins-enable-toggle` (`$plugin`), `plugins-invalid-manifest` (`$reason`),
`plugins-cpu` (`$pct`), `plugins-memory` (`$used`), `plugins-dash`,
`permission-<name>` × 25, `manifest-error-*` × 8,
`plugin-suspended` (`$plugin`, `$cause`), `plugin-suspended-cause-hang|
cpu-share|memory|did-not-start`, `plugin-auto-disabled` (`$plugin`),
`notification-action-restart-plugin`, `notification-action-disable-plugin`.

## 5. On-disk formats

- `plugins/bundled/<identifier>/plugin.toml`, `main.luau`, `README.md`
  (embedded; see [contracts/manifest.md](contracts/manifest.md)).
- `<plugin-state>/<hex(identifier)>/plugin.json`:
  `{ "version": 1, "entries": { "<key>": <json>, … } }`.
- `<plugin-state>/<hex(identifier)>/tracks/<hex(track)>.json`: same shape
  plus `"track": "<track id>"`.
- Marker file (006) — `owner` string now carries a plugin identifier;
  transient markers are never written. Loading an older file is unchanged
  (`owner` defaults to `"host"`).
