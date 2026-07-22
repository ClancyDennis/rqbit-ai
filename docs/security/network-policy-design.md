# NetworkPolicy design

- **Status:** Proposed for review
- **Scope:** Phase 2B policy seam and the Phase 2C private-torrent overlay
- **Implementation:** Not started; this document must be approved first
- **Baseline:** `main` at `5b82f52d`

## Decision summary

rqbit will route session-owned network activity through one policy-enforcing
runtime. `NetworkPolicy` defines the session's network mode. A separate
per-torrent `DiscoveryPolicy` controls which peer-discovery mechanisms a
torrent may use. The policy wrapper authorizes an operation before an
injectable backend may resolve a name, create a socket, connect a socket, or
construct an HTTP client.

The work is intentionally serialized at first:

1. land the policy types, recording backend, and contract tests;
2. land the production backend and convert `StreamConnector`;
3. only then convert independent call-site groups in parallel; and
4. implement private-torrent behavior after all discovery sources use the
   seam.

The default is `Normal`, which preserves current behavior. No existing
configuration silently becomes stricter.

## Goals

- Make every session-owned TCP, UDP, multicast, DNS, and HTTP egress decision
  observable at one seam.
- Make `ProxyOnly` fail closed rather than fall back to direct traffic.
- Make `LanOnly` reject public destinations before socket creation.
- Apply `bind_device` and IP-family rules consistently to permitted direct
  traffic.
- Express private-torrent discovery as an overlay on the same policy seam.
- Permit deterministic tests to prove that rejected operations never reach a
  DNS resolver or socket constructor.
- Preserve the library, CLI, HTTP server, and desktop application as separate
  consumers of `librqbit`.

## Non-goals

- Implement BEP 52.
- Proxy inbound peer connections or make SOCKS support UDP association.
- Treat a process-wide firewall or VPN as part of rqbit's correctness proof.
- Route the public HTTP API client through the torrent session policy.
- Promise that arbitrary third-party persistence drivers obey the policy.

## Current construction points

The current implementation does not have one enforceable network boundary.
The relevant production paths are:

| Activity | Current construction point | Current policy gap |
| --- | --- | --- |
| Outbound peer TCP | `StreamConnector::tcp_connect` in `crates/librqbit/src/stream_connect.rs` | Uses `BindDevice`, but SOCKS establishes its own connection to the proxy and accepts only pre-resolved peer addresses. |
| Outbound peer uTP | Shared `UtpSocketUdp` created by `ListenerOptions::start` | Direct UDP; enabled socket is also an inbound listener. |
| Incoming peer TCP/uTP | `ListenerOptions::start` in `crates/librqbit/src/listen.rs` | Independent of proxy expectations. |
| HTTP trackers and torrent URLs | Session `reqwest::Client` built in `Session::new_with_opts` | SOCKS content can be proxied while target DNS and interface/family guarantees remain incomplete. |
| UDP trackers | `UdpTrackerClient::new`; tracker names use `tokio::net::lookup_host` | Direct UDP and system DNS. |
| DHT | `DhtState::with_config`; bootstrap uses `tokio::net::lookup_host` | Direct bidirectional UDP and system DNS. |
| LSD | `LocalServiceDiscovery::new` | Direct IPv4/IPv6 multicast. |
| UPnP port forwarding | `UpnpPortForwarder`, `discover_services`, and `forward_port` | Direct SSDP plus separately constructed HTTP clients. |
| Blocklist/allowlist URLs | `IpRanges::load_from_url` | Calls `reqwest::get` outside the session client. |
| UPnP media advertisement/callback | `SsdpRunner::new` and `notify_system_id_update` | Direct multicast and subscriber-controlled HTTP callback egress. |
| CLI mDNS advertisement | `advertise_http_api` in `crates/rqbit/src/main.rs` | Independent LAN multicast daemon. |
| PostgreSQL persistence | `PostgresSessionStorage::new` | Direct control-plane connection outside the session networking stack. |

`librqbit-dualstack-sockets` 0.7 supplies the concrete bind/connect primitives,
but it is an external dependency and does not expose an injectable factory.
The policy layer will wrap it rather than fork it. If the factory abstraction
proves generally useful, it can later be upstreamed without making that a
prerequisite for rqbit.

## Public configuration

```rust
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    #[default]
    Normal,
    ProxyOnly,
    LanOnly,
}

#[derive(Debug, Clone)]
pub struct NetworkPolicy {
    pub mode: NetworkMode,
    pub proxy: Option<SocksProxyConfig>,
    pub bind_device: Option<BindDevice>,
    pub ipv4_only: bool,
}
```

`SessionOptions` receives `network_mode`, defaulting to `Normal`. Existing
fields remain accepted during a deprecation window:

- `connect.proxy_url` supplies the proxy endpoint;
- `bind_device_name` and `ipv4_only` become inputs to `NetworkPolicy`; and
- existing disable flags remain explicit feature switches inside `Normal`.

The CLI adds `--network-mode normal|proxy-only|lan-only` and
`RQBIT_NETWORK_MODE`. Desktop adds the same serialized field with a serde
default, so existing configurations deserialize as `Normal`.

Startup validation returns typed errors for contradictory configurations.
`ProxyOnly` requires a SOCKS5 proxy and rejects peer listeners, uTP, DHT, LSD,
UPnP forwarding, UPnP media serving, and mDNS advertisement. It does not
silently downgrade them. `LanOnly` rejects UPnP port forwarding and public
control-plane destinations.

## Policy and backend seam

The seam belongs in a small new lower-level workspace crate so that
`librqbit`, `dht`, `tracker_comms`, `upnp`, `upnp-serve`, and
`librqbit_lsd` can depend on it without dependency cycles. The working name is
`librqbit-network-policy`.

The crate contains no torrent/session state. It owns:

- `NetworkPolicy` and typed validation/denial errors;
- the proxy configuration/parser currently named `SocksProxyConfig` in
  `stream_connect.rs`;
- `EgressPurpose`, `Transport`, and target classification;
- `PolicyNetwork<B>`, which authorizes before calling `B`;
- a production backend wrapping `librqbit-dualstack-sockets`, Tokio DNS, and
  configured reqwest clients; and
- a recording backend used by contract tests.

The boundary is conceptually:

```rust
pub enum EgressPurpose {
    Peer,
    TrackerHttp,
    TrackerUdp,
    Dht,
    LocalServiceDiscovery,
    UpnpDiscovery,
    UpnpControl,
    UpnpMediaCallback,
    TorrentFile,
    IpFilterList,
    Mdns,
    Persistence,
}

pub enum Transport {
    Tcp,
    Udp,
    Multicast,
    Dns,
    Http,
}

pub trait NetworkBackend: Send + Sync {
    // Exact return types may be boxed adapters, but all raw operations live here.
    fn resolve(&self, request: ResolveRequest) -> BoxFuture<'_, Result<Vec<SocketAddr>>>;
    fn tcp_connect(&self, request: TcpConnectRequest) -> BoxFuture<'_, Result<TcpStream>>;
    fn bind_udp(&self, request: UdpBindRequest) -> Result<PolicyUdpSocket>;
    fn bind_multicast(&self, request: MulticastBindRequest) -> BoxFuture<'_, Result<PolicyMulticastSocket>>;
    fn http_client(&self, request: HttpClientRequest) -> Result<reqwest::Client>;
}
```

Call sites receive an `Arc<PolicyNetwork<_>>`, not a raw backend. The backend
methods are crate-private in production builds. A denied operation returns
before the backend method is invoked. The recording backend counts operations
and captures purpose, destination, bind device, and address family; it never
needs a real OS socket for denial tests.

The concrete UDP result remains compatible with
`librqbit_dualstack_sockets::UdpSocket`. uTP construction will use
`UtpSocket::new_with_opts` with that already-authorized socket instead of
calling `UtpSocketUdp::new_udp_with_opts`, which currently creates its own
socket internally.

HTTP is slightly different because reqwest owns its connector. Call sites may
only receive purpose-specific clients built by `PolicyNetwork`. `ProxyOnly`
clients normalize the legacy configured `socks5://` URL to remote target-name
resolution (`socks5h://`) internally. The proxy endpoint itself may require a
local lookup, but the requested tracker/torrent hostname must not. The proxy endpoint
connection receives the configured interface binding where the platform
supports it. Integration tests use a local SOCKS server and an origin trap to
prove that an unavailable proxy does not trigger direct fallback.

A CI source check will reject new production uses of:

- `tokio::net::lookup_host`;
- `reqwest::get`, `reqwest::Client::new`, and unconstrained
  `reqwest::Client::builder`;
- `librqbit_dualstack_sockets::{tcp_connect, UdpSocket::bind_udp,
  MulticastUdpSocket::new}`; and
- raw Tokio/socket2 network constructors outside the production backend and
  explicitly named listener/API modules.

This check is a guardrail, not the primary test. The policy contract tests are
the correctness proof.

## Mode semantics

| Operation | Normal | ProxyOnly | LanOnly |
| --- | --- | --- | --- |
| Outbound peer TCP | Direct or configured SOCKS, preserving current selection | SOCKS only; no direct fallback | Direct only after destination is classified LAN/local |
| Peer uTP | Allowed when configured | Rejected before UDP socket creation | Allowed only for LAN/local peers |
| Incoming peer listeners | Existing configuration | Rejected at startup | Allowed only on loopback/private/link-local binds |
| HTTP(S) trackers/torrent/filter URLs | Direct or configured proxy | SOCKS with proxy-side target DNS | Literal LAN/local destinations only in the first implementation |
| UDP trackers | Allowed when configured | Rejected | Literal LAN/local destination only; no system DNS |
| DHT | Existing behavior | Rejected | Rejected |
| LSD | Existing behavior | Rejected | Allowed |
| UPnP port forwarding | Existing behavior | Rejected | Rejected because it creates WAN exposure |
| UPnP media server | Explicit opt-in | Rejected | Explicit opt-in; callbacks restricted to LAN/local destinations |
| mDNS API advertisement | Explicit opt-in | Rejected | Explicit opt-in |
| Remote PostgreSQL | Existing behavior with warning if proxy mode is requested | Rejected unless a future separately reviewed connector is added | Rejected; local socket/loopback may be allowed |

Address classification is performed on `IpAddr`, not strings. LAN/local means
loopback, RFC1918 IPv4, IPv4 link-local, unique-local IPv6, or IPv6 link-local.
Unspecified, multicast, documentation, and public addresses are not generic
LAN destinations. Multicast is allowed only for an explicitly authorized
purpose such as LSD or mDNS.

`LanOnly` initially rejects ordinary hostnames. Resolving a hostname and then
filtering its result would already emit DNS traffic and would introduce DNS
rebinding races. A later `.local` resolver can be added as a separately tested
LAN mechanism.

## Bind-device and address-family contract

Every permitted direct TCP, UDP, and multicast request carries the normalized
bind device and `ipv4_only` decision from `NetworkPolicy`. The production
backend applies them before connect/send. It rejects an IPv6 destination under
`ipv4_only` before creating the socket.

Proxy traffic applies the bind device to the TCP connection to the proxy; the
remote target is not locally bound or resolved. `tokio-socks` supports
`connect_with_socket`, so `StreamConnector` will first obtain the
policy-created TCP stream to the proxy and then perform the SOCKS handshake on
that stream.

Platform limitations are explicit validation results. In particular, a mode
that promises an interface-bound proxy connection must fail closed on a
platform where the binding cannot be enforced; it must not log and continue.

## Torrent discovery overlay

Session network mode and torrent privacy are different axes:

```rust
pub enum DiscoveryPolicy {
    Public,
    Private,
    StrictMagnetPending,
}
```

- `Public` permits discovery sources allowed by the session mode.
- `Private` permits embedded torrent tracker tiers and established swarm peer
  connections. It denies DHT, LSD, PEX, and global trackers.
- `StrictMagnetPending` applies before metadata is known. It permits only the
  trackers embedded in the magnet and peers returned by those trackers.

The effective permission is the intersection of `NetworkPolicy` and
`DiscoveryPolicy`; neither can broaden the other. For example, a strict magnet
in `ProxyOnly` must have at least one embedded HTTP(S) tracker that the proxy
client can use. An embedded UDP tracker is not enough because `ProxyOnly`
denies UDP. If no embedded tracker is permitted by both policies, add-torrent
fails with a typed `NoPermittedMetadataSource` error before discovery starts.

Existing code already guards several known-private DHT/LSD and PEX paths, but
those checks are distributed. Phase 2C will express them through this overlay
and retain focused protocol tests.

Discovery streams receive a child cancellation token and a source label. If
an `AssumePublic` magnet resolves to metadata with `private=1`, rqbit must:

1. cancel the public DHT/LSD discovery streams immediately;
2. stop accepting further peers from those streams;
3. discard the remaining public discovery receiver rather than carrying it
   into the live private torrent; and
4. construct a new private receiver from embedded torrent trackers only.

This cannot undo disclosure that occurred before metadata was learned. The UI
and logs must not imply otherwise.

## Metadata-less magnet decision

Privacy cannot be inferred from a BTIH magnet before metadata arrives. The
backward-compatible default remains `AssumePublic`, with a clear warning when
public discovery is used for metadata resolution.

A new add-torrent option selects `strict_magnet_discovery`. Under this option:

- a magnet must contain at least one embedded `tr` tracker;
- global session trackers do not satisfy the requirement;
- DHT and LSD are never started for metadata discovery;
- a bare BTIH magnet is rejected synchronously with a typed error explaining
  that tracker-provided peers are required; and
- explicit initial peers do not silently weaken this rule. A future distinct
  trusted-peer option may be designed if there is a demonstrated need.

This behavior has a real UX cost, but it is the only mode that can claim it did
not first publish the unknown hash to public discovery. It is an explicit
choice rather than the default.

## Contract tests written before production conversion

The first implementation change adds these tests against `PolicyNetwork` and
the recording backend:

- `proxy_only_rejects_direct_udp_before_backend`
- `proxy_only_never_opens_direct_udp_socket`
- `proxy_only_rejects_multicast_before_backend`
- `proxy_only_rejects_direct_dns_before_backend`
- `proxy_only_peer_tcp_uses_only_proxy_endpoint`
- `proxy_only_has_no_direct_fallback_when_proxy_fails`
- `lan_only_rejects_public_ip_before_backend`
- `lan_only_rejects_hostname_before_resolver`
- `lan_only_allows_lsd_multicast`
- `ipv4_only_rejects_ipv6_before_backend`
- `bind_device_is_forwarded_to_direct_tcp`
- `bind_device_is_forwarded_to_direct_udp`
- `bind_device_applies_to_all_direct_egress`
- `normal_mode_preserves_existing_network_behavior`
- `strict_magnet_without_embedded_tracker_is_rejected`
- `strict_magnet_uses_only_embedded_trackers`

Integration tests added during call-site conversion include:

- `udp_tracker_proxy_only_does_not_create_client_socket`
- `dht_proxy_only_does_not_bind_or_resolve_bootstrap`
- `upnp_proxy_only_does_not_start_ssdp_or_http`
- `torrent_url_proxy_only_has_no_direct_http_fallback`
- `ip_filter_fetch_uses_session_http_policy`
- `private_magnet_transition_cancels_public_discovery`
- `private_torrent_never_queries_dht`
- `private_torrent_never_announces_lsd`
- `private_torrent_ignores_incoming_and_outgoing_pex`
- `private_torrent_uses_only_embedded_tracker_tiers`
- `private_torrent_resume_preserves_isolation`

Tests that assert no operation occurred must inspect the recording backend;
absence of packets on a best-effort test socket is not sufficient.

## Implementation sequence and safe fan-out

### Serialized core

One owner performs these changes as a single reviewed stack:

1. add the lower-level policy crate and typed errors;
2. write the recording backend and contract tests;
3. implement the production direct/proxy backend;
4. change `StreamConnector` and listener startup to use it; and
5. add startup validation and backward-compatible CLI/desktop configuration.

No call-site conversion starts until the interface and contract tests are
approved.

### Parallel call-site conversions

After the seam lands, disjoint owners may convert:

1. HTTP/UDP tracker communication;
2. DHT and LSD;
3. UPnP forwarding, UPnP media callbacks, and mDNS;
4. torrent URL plus blocklist/allowlist fetching; and
5. persistence validation for remote PostgreSQL.

Each conversion removes the old constructor from that component and adds its
named integration tests. Owners do not change the policy types.

### Phase 2C

Private-torrent work begins only after all discovery call sites use the seam.
It adds tracker-tier preservation, `DiscoveryPolicy`, strict magnet behavior,
transition cancellation, resume tests, and API/UI exposure. It does not create
a second networking abstraction.

## Compatibility and rollout

- `Normal` is the default for the library, CLI, and desktop.
- Existing proxy configuration in `Normal` retains current behavior but emits
  a precise warning that UDP/DHT/LAN services are direct.
- `ProxyOnly` and `LanOnly` are opt-in and reject incompatible settings with
  actionable messages.
- Deprecated individual flags remain supported for at least one release and
  are translated into `Normal` feature switches.
- API/config serialization uses defaults so older persisted desktop configs
  continue to load.
- No hard refusal is added to existing non-loopback HTTP API configurations as
  part of this work; that surface has its own authentication/deprecation plan.

## Acceptance gate

Phase 2B is ready to land only when:

- every session-owned production constructor is either routed through the
  seam or listed as an explicit, validated out-of-scope control plane;
- the source guard finds no unapproved raw constructor;
- all contract and converted call-site tests pass without requiring public
  network access;
- `Normal` compatibility tests pass;
- CLI and desktop configuration round-trip tests pass; and
- `cargo fmt --all -- --check`, `cargo check --workspace --locked`,
  `cargo clippy --all-targets --locked`, and `cargo test --workspace --locked`
  pass.

Phase 2C has a separate gate. In particular, Phase 2B must not claim that an
`AssumePublic` magnet later found to be private avoided its earlier public
discovery disclosure.
