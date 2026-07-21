# RFC: Desktop process isolation

- **Status:** Proposed for review
- **Decision:** None
- **Scope:** rqbit desktop architecture; no implementation is authorized by this RFC
- **Reviewed baseline:** repository HEAD `1fd0818e6efc1b48fd15b07fbc09ac8ad6e524cf`

All source locations in this RFC refer to that immutable baseline; later edits may shift local line numbers.

## Summary

The desktop application currently embeds `librqbit` in the Tauri native process. This is simple and fast, but the native process combines UI command handling, untrusted BitTorrent/network parsing, download-directory access, persistence, and optional HTTP serving.

This RFC compares that model with:

1. a Tauri shell connected to a separately sandboxed rqbit daemon over authenticated local IPC; and
2. a narrowly sandboxed worker that parses untrusted torrent metadata and returns a validated representation.

The recommendation is to build a time-boxed vertical-slice prototype of the daemon model and a minimal sandbox feasibility spike for the parser worker. Adoption remains gated on security review, cross-platform packaging evidence, compatibility, and the benchmark thresholds below. Process separation without enforceable privilege reduction is not sufficient reason to adopt either design.

## Current architecture

`State` holds an `Api` directly, and desktop startup constructs `Session` and `Api` in the Tauri native process (`desktop/src-tauri/src/main.rs:33-42`, `desktop/src-tauri/src/main.rs:67-144`). The same process can also bind and run the optional HTTP API (`desktop/src-tauri/src/main.rs:146-185`). Tauri commands call the in-memory `Api` directly for adding, listing, controlling, and deleting torrents (`desktop/src-tauri/src/main.rs:268-410`, `desktop/src-tauri/src/main.rs:435-457`).

The React renderer uses Tauri invoke for most control operations (`desktop/src/api.tsx:39-49`, `desktop/src/api.tsx:95-169`). Torrent files are currently base64-encoded in the renderer and decoded by the native process (`desktop/src/api.tsx:51-70`, `desktop/src/api.tsx:115-129`, `desktop/src-tauri/src/main.rs:332-346`). Logs and playlists can use the loopback HTTP API (`desktop/src/api.tsx:73-94`, `desktop/src/api.tsx:149-157`).

The native process therefore owns these authorities:

- peer, DHT, tracker, and UPnP networking through `Session`;
- the incoming peer listener, which defaults to wildcard TCP port 4240 with UPnP enabled (`desktop/src-tauri/src/config.rs:49-76`, `desktop/src-tauri/src/config.rs:80-91`);
- read/write access to configured download and persistence locations (`desktop/src-tauri/src/main.rs:74-84`, `desktop/src-tauri/src/config.rs:95-133`);
- desktop configuration file access (`desktop/src-tauri/src/main.rs:44-64`, `desktop/src-tauri/src/main.rs:189-220`); and
- an HTTP API that defaults to `127.0.0.1:3030` but can be configured differently (`desktop/src-tauri/src/config.rs:136-152`).

At the reviewed baseline, desktop HTTP options set no Basic Auth credential (`desktop/src-tauri/src/main.rs:88-92`), the Tauri CSP is null (`desktop/src-tauri/tauri.conf.json:23-35`), and the shell plugin is initialized (`desktop/src-tauri/src/main.rs:435-437`). The hardening change set following this baseline adds a restrictive CSP and removes the unused shell plugin; HTTP authentication policy remains separate follow-up work. Isolation must not be treated as a substitute for those controls.

The webview renderer may be hosted in a platform-managed process, but the trusted Tauri Rust backend and `librqbit` are one native process. A renderer compromise is constrained to exposed Tauri commands and other configured capabilities, yet those commands currently reach a high-privilege in-memory API. A parser panic, native dependency flaw, or resource-exhaustion failure in the networking engine can terminate the desktop backend because it shares that process.

## Goals

- Reduce the authority reachable from a compromised renderer.
- Contain crashes and native memory-safety defects triggered by untrusted network or torrent input.
- Give high-risk parsers only the resources they require.
- Preserve normal torrent throughput, streaming, incoming connections, and resume behavior.
- Preserve existing desktop configuration and persisted sessions where possible.
- Keep the library, CLI/server, web UI, and desktop products independently usable.

## Non-goals

- Replacing the public HTTP API with the internal IPC protocol.
- Treating localhost as an authentication boundary.
- Protecting against a fully compromised account with the same OS identity. Such a process can often inspect same-user data or inject UI input; IPC credentials alone do not solve that threat.
- Rewriting the BitTorrent engine as microservices.
- Adopting a design before prototype and security-review evidence exists.

## Assets and trust boundaries

The protected assets are downloaded content, the rest of the user's filesystem, tracker credentials and private metadata, persistence state, network identity, control of the torrent session, and host availability.

Relevant input classes are:

- renderer input, including a possible XSS or compromised frontend dependency;
- remote peer, tracker, DHT, UPnP, and HTTP responses;
- local or downloaded `.torrent` files and magnets;
- requests from another local OS user; and
- corrupted or older persistence/configuration data.

Every proposed process must have an explicit authority table:

| Component | Network | Filesystem | Session control | Untrusted parsing |
| --- | --- | --- | --- | --- |
| Current Tauri backend | Peer and optional HTTP networking | Config, persistence, download roots | Direct in-memory `Api` | Torrent and network inputs |
| Tauri shell with daemon | IPC only, plus platform UI services; no peer listener | UI configuration and user-selected paths only | Versioned, allowlisted IPC client | No peer/torrent parsing |
| rqbit daemon | Peer/tracker/DHT/UPnP and optional media endpoint | Explicit config, persistence, and download roots | Owns session | Network inputs; torrent input unless parser worker is also used |
| Parser worker | No network | No ambient filesystem access | None | Bounded metadata bytes only |

This table is an objective, not an automatic consequence of adding processes. It must be enforced with OS sandboxing, endpoint ACLs, restricted handles, and tests.

## Option 0: retain the in-process model

### Design

Keep `Session` and `Api` in Tauri. Harden the command surface, CSP, HTTP defaults, parsers, limits, and release pipeline without adding a process boundary.

### Advantages

- Lowest packaging, signing, startup, and lifecycle complexity.
- Direct Rust calls for control operations and direct access to session events.
- No new IPC authentication or protocol-version surface.
- No additional process memory, context switches, or control-message serialization.
- Current configuration, persistence, diagnostics, and HTTP behavior remain unchanged.

### Risks

- The Tauri backend retains peer networking, parsing, persistence, and download-directory authority.
- Engine failures terminate the desktop backend.
- Renderer-exposed command mistakes directly reach the privileged process.
- Platform sandboxing is coarse because the UI backend genuinely needs broad network and filesystem access.

This remains a valid outcome if the prototypes do not demonstrate material security benefit at acceptable cost.

## Option 1: authenticated local daemon IPC

### Process and privilege model

Bundle a version-matched `rqbit-desktop-daemon` sidecar. The daemon owns `Session`, persistence, peer sockets, trackers, DHT, UPnP, hashing, storage, and optional media/log streaming. The Tauri native process owns the window, file/folder pickers, configuration UI, and a small IPC bridge. The renderer continues to call a stable Tauri command surface; the native bridge translates those commands to daemon RPC and does not expose daemon credentials to JavaScript.

The daemon should receive only explicitly configured download and state directories. Where the platform allows it:

- macOS should use a signed helper with an App Sandbox profile or another reviewed sandbox mechanism and narrowly scoped file access;
- Windows should use a restricted token or AppContainer where compatible, a Job Object for lifecycle/resource limits, and a named-pipe ACL bound to the interactive user's SID; and
- Linux should combine a Unix-domain socket with owner-only permissions and a reviewed combination of Landlock/seccomp/namespaces or the packaging sandbox (for example, Flatpak portals).

If a platform implementation cannot prevent the daemon or Tauri shell from exercising the other's authority, the security benefit on that platform must be reported as fault isolation only.

### IPC transport and authentication

Prefer Unix-domain sockets on macOS/Linux and named pipes on Windows, not a fixed loopback TCP port. The prototype should use a length-delimited, versioned protocol with explicit maximum frame sizes. JSON can reuse current serde request/response types for the first prototype; binary frames should carry large bitfields rather than JSON arrays or base64.

Authentication must include both:

1. OS peer identity checks (`SO_PEERCRED`/equivalent, owner-only socket permissions, or Windows pipe SID validation); and
2. a random per-launch capability passed through an inherited pipe or handle, not command-line arguments, environment variables, renderer state, or logs.

The daemon must authenticate before parsing ordinary RPC bodies, negotiate an exact or compatible protocol version, authorize each method, and apply request-size and concurrency limits. Authentication does not grant arbitrary filesystem paths or generic command execution. Configuration and path changes remain separate, validated operations.

For a UI-owned daemon, the per-launch capability needs no disk persistence. A future daemon that intentionally survives UI exit would require a protected credential store, reconnect policy, and a separate threat review.

### Media and high-volume data

Control RPC should not carry torrent payload or streamed media. The daemon should read pieces directly from storage and serve media through one of two measured designs:

- a loopback HTTP endpoint with short-lived, random, path-scoped bearer capabilities minted through authenticated IPC; or
- a Tauri custom-protocol bridge that proxies daemon data.

The first design avoids an additional userspace copy through Tauri but adds a local HTTP security surface. The second reduces exposed listeners but may add copying, backpressure complexity, and renderer/backend coupling. The prototype must compare both. A public or LAN HTTP API remains a separately configured feature and must not share internal capability tokens.

### Lifecycle and upgrades

The initial prototype should be UI-owned:

- Tauri launches exactly one bundled daemon and holds a single-instance lock.
- The daemon signals readiness only after IPC authentication and persistence initialization.
- Tauri requests graceful shutdown, then uses platform process supervision to prevent an orphan.
- Unexpected daemon exit produces a visible degraded state; restart uses bounded exponential backoff and never silently starts a second writer over the same persistence store.
- Configuration changes that currently stop and recreate the in-process session (`desktop/src-tauri/src/main.rs:230-258`) become an explicit daemon transaction or controlled restart.

Desktop and sidecar ship as one signed/versioned unit. The handshake advertises protocol and persistence schema versions. An incompatible sidecar fails closed with an actionable error. Upgrade must be atomic from the user's perspective; rollback must either read the unchanged schema or refuse before mutating it. The daemon remains the only writer of torrent persistence.

### Security effect and limitations

This design can keep peer-network parsing and download-directory access out of the Tauri process, and a daemon crash need not destroy the UI. It does not automatically constrain a compromised renderer from invoking every command the normal UI may invoke. The Tauri bridge still needs the minimum command set, user-confirmation rules for destructive actions, request validation, CSP, and capability controls. The daemon also remains a large, network-facing trusted component with substantial filesystem authority.

## Option 2: sandboxed parser worker

### Process and privilege model

Keep the current Tauri/session placement, or place the worker behind the daemon if Option 1 is later adopted. Send only bounded `.torrent` or peer-metadata bytes to a helper over inherited anonymous pipes. The worker parses and validates a versioned metainfo representation, computes the required hashes, and returns normalized data plus explicitly required raw slices. It has:

- no listening socket or outbound network;
- no inherited download, configuration, or persistence directory handles;
- a read-only runtime image;
- CPU, wall-clock, address-space, and output-size limits; and
- a platform sandbox/restricted token that is tested rather than assumed.

The current parse boundary is compact: desktop input reaches `AddTorrent::TorrentFileBytes` (`desktop/src-tauri/src/main.rs:332-346`), and session parsing currently converts bytes to metainfo before creating storage (`crates/librqbit/src/session.rs:79-94`). This makes whole-metainfo parsing a feasible coarse-grained worker call.

Do not move per-peer wire messages or piece payload through this worker. Their frequency would make IPC overhead and ordering complex, and the worker would become another network engine. If peer-message parsing later needs containment, it requires a separate design.

### Authentication and validation

An inherited, non-listening pipe is the capability; no reusable bearer token is necessary. The parent must launch a bundled, signed/version-matched helper and close unrelated inherited handles. Messages remain length-delimited and versioned.

Worker output is untrusted at the parent boundary. The parent must validate lengths, hash identities, file counts, path components, and storage-root policy before opening files. The worker must never return an instruction equivalent to “open this arbitrary path.”

### Lifecycle and upgrades

A warm worker avoids a process spawn per torrent. It can be restarted after a crash or limit violation, with the triggering input rejected. The helper and parent must have an exact schema handshake and ship atomically. Because the worker owns no persistent state, rollback and crash recovery are simpler than the daemon model.

### Security effect and limitations

This is the smallest boundary around hostile metainfo and can contain parser crashes, memory corruption, and resource exhaustion if the OS limits work. It adds no media-streaming overhead. It does not isolate peer protocol, tracker, DHT, UPnP, hashing/storage, or the Tauri command surface. Rust memory safety also means the incremental security value must be demonstrated against actual unsafe/native dependencies and availability threats, not assumed.

## Comparison

| Criterion | Current in-process | Local daemon | Parser worker |
| --- | --- | --- | --- |
| Renderer-to-engine boundary | Tauri commands to in-memory API | Tauri commands, then authenticated RPC | Unchanged |
| Peer/network failure containment | Tauri backend exits | UI can survive daemon exit | Unchanged |
| Torrent parser containment | None beyond Rust/task boundaries | Only if daemon isolation is sufficient; UI survives | Strongest and narrowest when sandboxed |
| Download-directory authority in Tauri | Yes | No, if enforced | Yes unless combined with daemon |
| Control-path latency | Lowest | Serialization and context switch | Unchanged after parsing |
| Streaming path | Direct in process/HTTP | Direct daemon HTTP or proxied custom protocol | Unchanged |
| Extra copying | Current base64 upload already copies | Small control messages; media design-dependent | Metadata input/output only |
| Lifecycle/state complexity | Lowest | Highest | Moderate, stateless worker |
| Packaging/signing cost | Current baseline | Sidecar, IPC, sandbox, supervision on three OSes | Helper and sandbox on three OSes |
| Backward-compatibility risk | Lowest | IPC/config/lifecycle migration | Metainfo serialization boundary only |
| Security scope | Hardening only | Broad process and privilege separation | Narrow parser isolation |

## Migration and backward compatibility

Any prototype must preserve the existing UI-facing `RqbitAPI` contract and Tauri command names so the shared web UI does not need an architecture-specific fork (`desktop/src/api.tsx:73-169`).

For a daemon migration:

- reuse the current desktop config location and deserialize the existing schema before adding versioned fields;
- preserve the existing persistence folder and torrent bytes; the daemon becomes their sole writer;
- preserve existing peer port, UPnP, SOCKS, DHT, rate-limit, and HTTP API settings;
- keep the public HTTP API opt-in behavior distinct from internal IPC;
- provide a developer-only or release-scoped fallback to the in-process backend during evaluation, with an explicit warning and telemetry-free diagnostics; and
- do not automatically attach to an arbitrary existing `rqbit server` process. That server has different ownership and authentication assumptions.

For a parser-worker migration, maintain byte-for-byte raw info-dictionary handling where hash identity depends on original bencoding. Run old and new parsing in differential tests before switching the default. On disagreement, reject the torrent during the evaluation period rather than silently choosing one result.

Destructive schema migration is out of scope. Before adoption, test upgrade and rollback from at least the two latest released desktop versions on all supported platforms.

## Prototype plan and benchmarks

### Daemon vertical slice

Implement in an experimental branch only:

1. sidecar launch, authenticated version handshake, health, and shutdown;
2. `torrents_list`, one stats/event stream, add-by-URL, pause/start, and one destructive operation;
3. binary transfer of a representative “haves” bitfield;
4. existing persistence opened exclusively by the daemon; and
5. both direct capability-HTTP and proxied custom-protocol media paths.

Measure current and prototype builds on Windows, macOS, and Linux, using the same fixture and release optimization:

- cold start to first successful torrent list: p50 and p95;
- p50/p95 RPC latency for list, stats, pause/start, and 1 KiB/1 MiB bitfields;
- CPU, combined RSS, context switches, and open handles at idle and with 100 active torrents;
- add latency and peak memory for small, 1 MiB, and maximum-allowed torrent metadata;
- sustained sequential and seek-heavy streaming of a 1 GiB local file;
- download and seed throughput with 1, 50, and 500 peers where the test environment permits;
- daemon crash, restart, and persistence recovery time; and
- installer size, startup failures, signing/notarization behavior, and upgrade/rollback results.

Proposed performance budgets for adoption review:

- common control RPC p95 at or below 25 ms and no more than 10 ms slower than the current path;
- cold-start regression no greater than 500 ms p95;
- direct streaming throughput at least 95% of current, with CPU no more than 110% of current;
- combined steady-state RSS no more than 75 MiB above current;
- no torrent piece payload routed through the Tauri process; and
- no statistically significant download/upload throughput regression greater than 5% in repeated local tests.

These are review thresholds, not product guarantees. The benchmark report must include hardware, OS, fixture, sample count, variance, and raw results.

### Parser-worker spike

Prototype one bounded parse request using existing valid and malformed corpora. Measure warm/cold latency, peak worker memory, bytes copied, and throughput for small, 1 MiB, and maximum-allowed metadata. Force panic, timeout, malformed output, oversized output, and abrupt exit.

Proposed budgets are a warm-parser overhead below 15% for maximum-sized metadata, below 10 ms p95 for typical metadata, and no measurable torrent payload-streaming regression. If platform sandbox policies cannot deny both filesystem and network access, the spike fails its security objective regardless of speed.

## Security acceptance tests

The daemon prototype must demonstrate:

- another OS user cannot connect to the socket/pipe;
- same-user connection without the per-launch capability is rejected before RPC decoding;
- credentials never appear in process arguments, environment, renderer JavaScript, logs, crash reports, or persistent config;
- incompatible protocol versions and oversized/truncated frames fail closed;
- only allowlisted methods exist, and malformed parameters cannot select arbitrary filesystem paths or execute commands;
- a forged, expired, replayed outside its scope, or wrong-path media capability is rejected;
- public/LAN HTTP API credentials cannot authenticate internal IPC, and internal credentials cannot authenticate public HTTP;
- renderer compromise tests cannot obtain the daemon credential or bypass Tauri-side confirmation for designated destructive actions;
- daemon crash/hang leaves the UI responsive, releases locks after termination, and does not corrupt persistence;
- UI crash follows the documented daemon ownership policy and cannot leave an uncontrolled sidecar;
- update, downgrade, and mismatched-sidecar tests fail without mutating state; and
- platform tests prove the intended network/filesystem restrictions, not merely process separation.

The parser-worker prototype must demonstrate:

- attempts to open a file, enumerate a directory, create a socket, resolve DNS, or spawn a process are denied;
- memory, CPU, wall-clock, input-size, and output-size limits terminate or reject abusive input;
- panic, abort, malformed response, timeout, and worker disappearance reject the torrent without terminating the parent;
- parent-side validation rejects traversal, invalid lengths, duplicate paths, inconsistent hashes, and unknown schema versions; and
- the existing parser corpus plus fuzz-generated cases produce equivalent accepted metadata and hashes, or an explicitly reviewed difference.

## Decision criteria

Adopt the daemon architecture only if all of the following are true:

1. The threat model and security review identify a material reduction in Tauri-process authority on every supported platform.
2. OS-enforced tests show that Tauri cannot access download/persistence data and the daemon cannot access UI-only resources beyond its grants.
3. IPC authentication, method authorization, media capabilities, lifecycle, and upgrade tests pass.
4. Existing configuration, persistence, public API behavior, incoming connectivity, and rollback remain compatible.
5. The prototype meets the performance budgets or reviewers explicitly accept a measured exception.
6. Packaging, code signing/notarization, crash reporting, and support burden are sustainable.

Reject or defer the daemon if it only moves code to another unrestricted same-user process, requires exposing an unauthenticated loopback control port, weakens streaming materially, creates dual persistence writers, or cannot upgrade atomically.

Adopt the parser worker only if the sandbox denial tests pass on every supported platform, parent-side revalidation is complete, differential tests establish correctness, and measured overhead meets its budgets. Reject it if it becomes a general network/piece-processing service or if it cannot be meaningfully sandboxed.

The two designs are composable: an adopted daemon may later use a parser worker. They should still receive separate decisions because their threat coverage, operational cost, and failure modes differ.

## Recommendation

Approve prototypes, not adoption.

The daemon vertical slice is worth measuring because it is the only option here that can remove peer networking and download-directory authority from the Tauri backend and let the UI survive engine failure. The parser-worker spike is worth doing in parallel only as a small feasibility study because it offers a much narrower, easier-to-test boundary and does not affect streaming.

After the prototype reports, reviewers should choose among retaining the hardened in-process model, adopting only parser isolation, adopting the daemon, or combining them. Until the explicit criteria above are met, the current architecture remains the supported design.
