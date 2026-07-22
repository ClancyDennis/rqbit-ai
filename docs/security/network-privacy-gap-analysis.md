# Networking and privacy gap analysis

Scope: repository HEAD `1fd0818e6efc1b48fd15b07fbc09ac8ad6e524cf`.

All source locations refer to that immutable baseline; later edits may shift local line numbers.

The proposed implementation contract derived from this analysis is in
[`network-policy-design.md`](network-policy-design.md).

This analysis distinguishes traffic that is proxied, traffic that is intentionally local, and traffic that bypasses the configured SOCKS proxy or bind policy. “Leak” below means traffic that contradicts a reasonable proxy-only or interface-only expectation; it does not mean that all direct BitTorrent traffic is a defect in Normal mode.

## Current defaults

`SessionOptions` enables DHT and local service discovery (LSD), enables trackers, permits IPv4 and IPv6, and has no SOCKS proxy or device binding by default (`crates/librqbit/src/session.rs:418-482`, `crates/librqbit/src/session.rs:484-510`). A library caller must opt into a peer listener.

The CLI server adds a dual-stack TCP peer listener on port 4240, enables UPnP port forwarding, and exposes the HTTP API only on `127.0.0.1:3030` unless configured otherwise (`crates/rqbit/src/main.rs:615-628`, `crates/rqbit/src/main.rs:729-780`). One-shot download uses an ephemeral listener and does not request UPnP forwarding unless explicitly enabled (`crates/rqbit/src/main.rs:789-812`).

Desktop defaults differ: it listens for TCP peers on IPv4 wildcard port 4240, enables UPnP forwarding, disables uTP, enables DHT/LSD through session defaults, and runs a writable HTTP API on `127.0.0.1:3030` (`desktop/src-tauri/src/config.rs:32-93`, `desktop/src-tauri/src/config.rs:136-152`, `desktop/src-tauri/src/main.rs:106-138`). Desktop does not expose a bind-device or IPv4-only setting.

The CLI currently describes `--socks-url` as applying to “all outgoing connections,” while separately suggesting that users disable incoming TCP (`crates/rqbit/src/main.rs:240-245`). The implementation does not satisfy the first statement.

## Traffic-path matrix

| Path | Direction and activation | SOCKS behavior | Bind-device / IP-family behavior | Finding |
| --- | --- | --- | --- | --- |
| Peer TCP | Outbound whenever peers are discovered; inbound when TCP listener enabled | Outbound peer connections use SOCKS first and return without direct TCP fallback when a proxy is configured (`crates/librqbit/src/stream_connect.rs:210-249`). Inbound peers never traverse SOCKS. | Direct outbound and inbound sockets receive `BindDevice`; `ipv4_only` rejects IPv6 peers (`crates/librqbit/src/stream_connect.rs:186-225`, `crates/librqbit/src/listen.rs:77-115`). The TCP connection to the SOCKS server itself is created by `tokio-socks` without the configured `BindDevice` (`crates/librqbit/src/stream_connect.rs:88-109`). | Peer payload is proxied outbound, but a configured listener still exposes the host address. Proxy plus bind-device is not fail-closed on the chosen interface. |
| Peer uTP | Outbound and inbound only when a uTP listener/socket is enabled | SOCKS takes precedence, so peer connections use SOCKS/TCP rather than uTP when configured. Without SOCKS, uTP sends direct UDP (`crates/librqbit/src/stream_connect.rs:228-238`, `crates/librqbit/src/stream_connect.rs:240-324`). | The shared uTP socket is created with `BindDevice`; `ipv4_only` rejects IPv6 peer destinations (`crates/librqbit/src/listen.rs:120-147`, `crates/librqbit/src/stream_connect.rs:221-225`). | No SOCKS UDP support exists. Strict proxy mode must disable uTP rather than fall back to it. |
| HTTP/HTTPS trackers | Outbound for configured torrent/global trackers; enabled by default | Uses the session `reqwest::Client`, which receives `Proxy::all(proxy_url)` (`crates/librqbit/src/session.rs:693-725`, `crates/tracker_comms/src/tracker_comms.rs:319-353`). The only accepted project proxy scheme is `socks5`, not `socks5h` (`crates/librqbit/src/stream_connect.rs:70-85`). With locked reqwest 0.12.28 (`Cargo.lock:4631-4635`), `socks5` resolves the target hostname locally; only `socks5h` requests proxy-side DNS. | Without SOCKS, reqwest binds to the named interface only on non-Windows (`crates/librqbit/src/session.rs:711-719`). DNS resolution is not constrained by that interface setting. `ipv4_only` is not applied to reqwest. | HTTP contents are proxied, but tracker hostname DNS leaks locally. Interface and IPv4-only semantics are incomplete. |
| UDP trackers | Outbound for every `udp://` tracker; enabled by default | Never uses SOCKS. Tracker hostnames are resolved with the system resolver, then packets are sent directly (`crates/tracker_comms/src/tracker_comms.rs:161-194`, `crates/tracker_comms/src/tracker_comms.rs:377-444`). | The UDP client socket receives `BindDevice`, but is always created from IPv6 unspecified and can use both families; it receives no `ipv4_only` setting (`crates/tracker_comms/src/tracker_comms_udp.rs:267-295`). DNS remains outside socket binding. | **Confirmed SOCKS and IPv4-only bypass.** |
| DHT | Bidirectional UDP; enabled by default for the session and public torrents | Never uses SOCKS. Bootstrap hostnames use the system resolver and DHT datagrams are direct (`crates/dht/src/dht.rs:981-1025`, `crates/dht/src/dht.rs:1148-1181`). | DHT bind address follows `ipv4_only` and the UDP socket receives `BindDevice` (`crates/dht/src/persistence.rs:33-46`, `crates/dht/src/dht.rs:1276-1313`). Bootstrap DNS is not interface-bound. | **Confirmed SOCKS bypass.** It also creates an inbound UDP listener independent of the peer listener. |
| LSD | Bidirectional IPv4/IPv6 multicast on port 6771; enabled by default, but suppressed for known private torrents | Never uses SOCKS (`crates/librqbit_lsd/src/lib.rs:21-28`, `crates/librqbit_lsd/src/lib.rs:84-113`, `crates/librqbit_lsd/src/lib.rs:218-291`). | Receives `BindDevice`. It always constructs IPv4 and IPv6 multicast sockets and receives no `ipv4_only` setting. | **Confirmed local multicast and IPv4-only bypass.** This is LAN disclosure rather than Internet egress. |
| UPnP port forwarding | Outbound SSDP plus router HTTP/SOAP; on by default for CLI server and desktop peer listeners | Never uses SOCKS. SSDP discovery is direct multicast. Device-description and SOAP clients are separately constructed direct reqwest clients (`crates/upnp/src/lib.rs:379-420`, `crates/upnp/src/lib.rs:327-343`, `crates/upnp/src/lib.rs:138-189`). | SSDP receives `BindDevice`; device-description and SOAP HTTP requests do not. Discovery is IPv4-only. The requested mapping is TCP only (`crates/upnp/src/lib.rs:138-159`). | **Confirmed SOCKS bypass.** Also a bind-device bypass after discovery. Enabling uTP does not create a UDP mapping. |
| mDNS HTTP-API advertisement | Bidirectional LAN multicast; CLI opt-in and only valid with a non-loopback HTTP API | Never uses SOCKS. `mdns_sd::ServiceDaemon` advertises the bound API address and auto-publishes host addresses for wildcard binds (`crates/rqbit/src/main.rs:1030-1034`, `crates/rqbit/src/main.rs:1097-1160`). | No session bind-device or `ipv4_only` policy is passed. | Explicit LAN disclosure. Must be disabled in proxy-only mode and opt-in in LAN-only mode. |
| HTTP API / Web UI | Inbound TCP; CLI server defaults loopback, desktop defaults loopback, non-loopback is explicit | SOCKS is irrelevant to an inbound listener. | HTTP listeners use default bind options, not the peer bind-device (`crates/rqbit/src/main.rs:999-1028`, `desktop/src-tauri/src/main.rs:146-184`). The configured `SocketAddr` selects family/interface. | Safe from network exposure by default, but a non-loopback bind is an independent control/data surface and must not be silently enabled by a network mode. |
| Torrent URL fetch | Outbound when adding an `http(s)://` torrent | Uses the session reqwest client, so the HTTP connection is proxied; `socks5` target DNS remains local (`crates/librqbit/src/session.rs:156-172`, `crates/librqbit/src/session.rs:706-725`). | Same non-Windows-only reqwest interface behavior; `ipv4_only` is not applied. | HTTP content is proxied, but DNS, interface, and family guarantees are incomplete. |
| Blocklist/allowlist fetch | Outbound only when URL options are configured | Bypasses the session client and calls `reqwest::get` directly (`crates/librqbit/src/session.rs:739-759`, `crates/librqbit/src/ip_ranges.rs:61-81`). | Ignores bind-device and `ipv4_only`. `file://` is local and does not create traffic. | **Confirmed SOCKS, bind-device, and IPv4-only bypass.** The resulting lists filter peer IPs only, not tracker/DHT/HTTP traffic (`crates/librqbit/src/session.rs:904-930`, `crates/librqbit/src/torrent_state/live/mod.rs:600-649`). |
| UPnP media server SSDP and event callbacks | LAN listener/multicast and outbound callbacks; explicit opt-in, requires non-loopback HTTP API | SSDP uses its own multicast socket with no proxy. A subscriber-supplied callback URL is later requested with a new direct reqwest client (`crates/upnp-serve/src/ssdp.rs:104-130`, `crates/upnp-serve/src/subscriptions.rs:107-153`, `crates/upnp-serve/src/services/content_directory.rs:228-258`). | The SSDP runner passes no `BindDevice`; callbacks ignore bind-device and IP-family policy. | **Confirmed opt-in bypass and callback/SSRF-capable egress path.** LAN-only must validate callback destinations and redirects; proxy-only must disable this server. |
| PostgreSQL persistence | Outbound database connection; optional feature and explicit `postgres://` configuration | Uses sqlx directly, not SOCKS (`crates/librqbit/src/session_persistence/postgres.rs:49-56`). | Ignores bind-device and `ipv4_only`. | Operator control-plane traffic, but a strict proxy-only profile must reject remote PostgreSQL or require a separately acknowledged exception. |

Initial peers and peers learned from trackers, DHT, LSD, or PEX all converge on the same `StreamConnector`; therefore an initial peer is proxied when SOCKS is configured. The discovery mechanism that supplied the address may still have leaked directly.

## Confirmed privacy gaps

### SOCKS is not a network-wide privacy control

With `--socks-url`, direct UDP tracker, DHT, LSD, and UPnP packets remain active. Default CLI server and desktop settings additionally leave an incoming peer listener and UPnP TCP mapping enabled. HTTP trackers and torrent downloads use SOCKS but resolve target hostnames locally because rqbit accepts only `socks5`, not `socks5h`. Optional block/allowlist downloads bypass SOCKS completely.

There is no direct fallback after a SOCKS peer connection fails, which is good (`crates/librqbit/src/stream_connect.rs:228-238`). The problem is that several independent subsystems never enter `StreamConnector` or the session HTTP client.

### Bind-device is broad in documentation, partial in implementation

The CLI says bind-device applies to “DHT, BT-UDP, BT-TCP, trackers and LSD” (`crates/rqbit/src/main.rs:216-221`). It does cover peer sockets, DHT, UDP trackers, LSD, and UPnP SSDP. It does not cover:

- the peer TCP connection to the SOCKS server;
- DNS queries;
- HTTP traffic on Windows;
- block/allowlist fetches;
- UPnP device-description and SOAP HTTP;
- mDNS or UPnP media SSDP/callbacks;
- HTTP API listeners; or
- PostgreSQL persistence.

Bind-device should be documented as route/interface selection, not a privacy kill switch, until every policy-relevant dialer is centralized and fail-closed.

### IPv4-only is subsystem-specific

`ipv4_only` constrains peer destinations/listeners and DHT, but not reqwest, UDP tracker resolution/socket use, LSD IPv6 multicast, mDNS, or auxiliary fetches. A user can therefore observe IPv6 DNS/HTTP/UDP traffic with `--ipv4-only`.

### Private torrents have two concrete discovery leaks

For a known private `.torrent`, rqbit suppresses DHT and LSD (`crates/librqbit/src/session.rs:1511-1551`) and suppresses PEX sending/processing and metadata responses (`crates/librqbit/src/torrent_state/live/mod.rs:1083-1110`, `crates/librqbit/src/torrent_state/live/mod.rs:1179-1195`). However:

1. When a private torrent has zero or one embedded tracker, globally configured trackers are appended because the private special-case is entered only when `trackers.len() > 1` (`crates/librqbit/src/session.rs:1553-1565`). This can disclose a private info hash to an unrelated tracker.
2. A metadata-less magnet is considered non-private before resolution, so DHT and LSD can be used before the downloaded metadata reveals the private flag (`crates/librqbit/src/session.rs:1218-1260`).

Private torrents with multiple trackers are instead truncated to the first tracker, which avoids global tracker extension by accident but loses tier/failover behavior. The extended handshake also advertises metadata size without checking the private flag (`crates/librqbit/src/torrent_state/live/mod.rs:1234-1245`), although a later metadata request is refused.

## Proposed policy model

Implement one session-level transport profile, `Normal`, `ProxyOnly`, or `LanOnly`. `Private` is a per-torrent overlay derived from validated metadata; it composes with any transport profile. This avoids pretending that “private torrent” and “SOCKS transport” are mutually exclusive.

Policy must be evaluated before constructing sockets, resolving hostnames, following redirects, or starting background services. A central `NetworkPolicy` should create all resolvers, HTTP clients, listeners, UDP sockets, and outbound connectors. Subsystems must not create independent reqwest clients.

### Normal

Normal preserves current compatibility:

- direct TCP/uTP peers, HTTP/UDP trackers, DHT, and LSD are permitted as configured;
- incoming peer listeners and UPnP mapping are permitted as configured;
- SOCKS remains usable as a best-effort peer and HTTP proxy, but startup emits a precise warning listing enabled bypass paths;
- mDNS, UPnP media serving, non-loopback HTTP API, block/allowlist URLs, and remote PostgreSQL remain explicit opt-ins; and
- IPv4-only and bind-device retain their documented subsystem limitations until centralized.

Normal is the migration default. It is not marketed as anonymity or leak prevention.

### Private overlay

For a torrent whose validated info dictionary has `private=1`:

- use only the torrent's tracker tiers; never append session-global or arbitrary public trackers;
- allow an explicitly added tracker only through an operation labeled as a private-torrent tracker override;
- disable DHT and LSD lookup/announcement;
- do not advertise, send, accept, or act on PEX;
- do not advertise or serve metadata exchange;
- allow peers returned by the private trackers, explicitly supplied initial peers, and incoming peers matching the torrent hash; and
- apply the active transport profile to tracker and peer sockets.

A metadata-less magnet cannot establish the private bit without first performing discovery/metadata exchange. Guaranteed Private mode must reject such magnets and require a `.torrent` file. An optional future `assume_private` input may restrict discovery to explicit trackers, but it still must not use peer metadata exchange unless the private-tracker interoperability policy explicitly permits it.

### Proxy-only

Proxy-only is fail-closed and requires a valid SOCKS5 proxy:

- allow outbound peer TCP only through SOCKS; never fall back direct;
- allow HTTP/HTTPS trackers, torrent URLs, and HTTP block/allowlist URLs only through a shared proxy client using proxy-side target DNS (`socks5h` semantics);
- reject or skip UDP trackers before hostname resolution, with a visible per-torrent error;
- disable DHT, LSD, uTP, incoming peer listeners, UPnP port forwarding, mDNS, and the UPnP media server;
- permit the HTTP API only on loopback; a non-loopback API is a configuration error in this mode;
- apply bind-device, when requested, to the connection to the SOCKS server and fail startup if the platform cannot enforce it;
- send no direct IPv4 or IPv6 Internet traffic other than connecting to the configured proxy endpoint;
- announce no fabricated listening port. Use `port=0`/outbound-only semantics where trackers support it and report incompatible trackers rather than silently advertising 4240; and
- reject remote PostgreSQL persistence unless the user supplies a separately named, explicit direct-egress exception. Local Unix-domain persistence is allowed.

Proxy failure is a torrent/session error, never a reason to retry directly. Proxy-only does not promise protection from a malicious proxy, traffic correlation, browser traffic, or a compromised host.

### LAN-only

LAN-only permits local collaboration without global Internet destinations:

- allow peer TCP/uTP only to or from IPv4 loopback, RFC1918, and link-local addresses, and IPv6 loopback, link-local, and ULA addresses;
- exclude globally routable and carrier-grade NAT (`100.64.0.0/10`) destinations by default;
- enable LSD; disable global DHT and UPnP port forwarding;
- permit HTTP/UDP trackers and torrent/list URLs only when every resolved address and every redirect target is in the allowed LAN set;
- permit mDNS HTTP-API advertisement and UPnP media SSDP only as explicit opt-ins;
- require a non-loopback HTTP API to bind a selected LAN address/interface, use authentication, and reject requests from non-LAN source addresses;
- restrict UPnP event callback URLs, resolved addresses, and redirects to the LAN set;
- bind all multicast and unicast sockets to the selected interface when supported; and
- reject public peers received through PEX, trackers, initial-peer input, or incoming sockets before connection/handshake.

Hostnames may use the system's LAN resolver, but destination classification happens after resolution and again after every redirect. `file://` inputs remain allowed. LAN-only is an address-scope guarantee, not protection against hostile devices on the LAN.

## Router, NAT, and CGNAT implications

Outbound connections work through normal NAT without a port mapping. Incoming reachability requires at least one of:

- a working mapping on every NAT layer;
- a manually forwarded port to a stable LAN address; or
- globally reachable IPv6 with host/router firewall permission.

Current UPnP maps only TCP even when the listener also enables uTP/UDP (`crates/upnp/src/lib.rs:138-159`). It controls only a discovered local gateway. A successful immediate-router response does not prove reachability through double NAT or ISP CGNAT. Under CGNAT, the subscriber generally cannot create the upstream mapping; downloads still work outbound, but incoming connections and seeding reachability are reduced. NAT-PMP/PCP are not implemented.

The CLI peer listener is dual-stack by default (`crates/rqbit/src/main.rs:194-196`, `crates/librqbit/src/listen.rs:89-115`). Globally addressed IPv6 may bypass the IPv4 NAT assumption and be reachable subject to firewall policy. Desktop's peer listener is currently IPv4 wildcard while its DHT and outgoing paths can still use IPv6.

DHT owns a separate UDP listen port and is not covered by the peer TCP UPnP mapping. Proxy-only deliberately gives up incoming reachability. LAN-only permits incoming peers only from LAN address classes and never asks the router for an external mapping.

Connectivity UI/diagnostics should report the bound TCP/uTP/DHT addresses, selected interface, mapped protocol/port, mapping gateway, observed external address when known, IPv4/IPv6 status, and “mapping created” separately from “externally reachable.” CGNAT detection is heuristic and must be labeled as such.

## Compatibility and deprecation

1. Add a versioned `network_mode` field defaulting to `normal`; old desktop and persisted configurations deserialize to Normal without changing traffic.
2. In the first release, retain `--socks-url` behavior but replace “all outgoing connections” documentation with “peer TCP and session HTTP only,” and warn with the exact active bypasses.
3. Add `--network-mode proxy-only` as the explicit leak-prevention contract. Reject conflicting flags rather than silently overriding them. Desktop should show the services that will be disabled before applying it.
4. Do not silently reinterpret `--socks-url` as Proxy-only in the same release: disabling DHT/UDP trackers/incoming peers can break magnet resolution, tracker availability, and seeding.
5. Fix private global-tracker merging immediately as a security correction. Release notes should call out that private torrents will no longer receive session-global trackers and that metadata-less magnets cannot provide a strict Private guarantee.
6. Keep LAN API/mDNS/UPnP media settings separate from the torrent transport profile; migrations must not expose the API automatically.
7. Clarify `--ipv4-only` and `--bind-device` limitations immediately. Expanding them to all subsystems is desirable but may change routing, DNS, and multi-homed deployments; stage that behind the central policy implementation.
8. Preserve an explicit Normal mode for users who intentionally combine SOCKS peers with direct DHT/LSD/UDP trackers. Security warnings must be suppressible only by choosing and acknowledging Normal, not by claiming leak-free operation.

## Named tests

### Policy matrix unit tests

Add `crates/librqbit/src/tests/network_policy.rs` with a fake resolver/socket/HTTP factory and these named cases:

- `normal_preserves_current_enabled_paths`
- `proxy_only_requires_proxy`
- `proxy_only_socks_failure_never_falls_back_direct`
- `proxy_only_peer_tcp_uses_socks`
- `proxy_only_rejects_udp_tracker_before_dns`
- `proxy_only_starts_no_dht_socket_or_bootstrap_dns`
- `proxy_only_starts_no_lsd_upnp_or_mdns`
- `proxy_only_rejects_non_loopback_http_api`
- `proxy_only_torrent_url_uses_proxy_dns`
- `proxy_only_blocklist_uses_session_proxy_client`
- `proxy_only_ipv6_destination_never_dials_direct`
- `proxy_only_does_not_advertise_default_port_without_listener`
- `bind_device_applies_to_proxy_endpoint`
- `ipv4_only_rejects_ipv6_udp_tracker_before_send`
- `lan_only_allows_rfc1918_link_local_and_ula_peers`
- `lan_only_rejects_global_and_cgnat_peers`
- `lan_only_rejects_redirect_from_lan_to_public_address`
- `lan_only_rejects_public_incoming_peer`
- `lan_only_never_creates_router_mapping`
- `lan_only_upnp_callback_must_resolve_to_lan`

### Private-torrent tests

Add `crates/librqbit/src/tests/private_network_policy.rs`:

- `private_zero_tracker_does_not_add_global_trackers`
- `private_one_tracker_does_not_add_global_trackers`
- `private_multiple_tracker_tiers_are_preserved`
- `private_never_queries_or_announces_dht`
- `private_never_announces_lsd`
- `private_never_advertises_or_serves_metadata`
- `private_never_sends_or_accepts_pex`
- `private_metadata_less_magnet_is_rejected_in_strict_mode`
- `private_initial_peer_obeys_transport_profile`

### Socket-level no-leak integration tests

Run Linux network-namespace tests in CI, with portable mock equivalents on macOS/Windows:

- `proxy_only_packet_capture_sees_only_proxy_endpoint`
- `proxy_only_dns_canary_receives_no_target_queries`
- `proxy_only_udp_tracker_canary_receives_nothing`
- `proxy_only_direct_route_blackhole_does_not_change_behavior`
- `lan_only_public_sink_receives_nothing`
- `bind_device_interface_loss_fails_closed`
- `normal_mode_direct_peer_regression`

The proxy test should provide fake SOCKS, HTTP tracker, UDP tracker, DHT bootstrap, DNS, and peer endpoints in one fixture. Assert on attempted socket creation and captured packets, not only successful application calls.

### Router tests

Extend the mocked IGD suite with:

- `upnp_maps_tcp_when_tcp_listener_enabled`
- `upnp_maps_udp_when_utp_listener_enabled`
- `upnp_does_not_map_http_api_port`
- `upnp_success_is_not_reported_as_external_reachability`
- `double_nat_or_cgnat_status_is_indeterminate`

The UDP mapping test is expected to fail until uTP mapping support is implemented. Tests should not claim that any synthetic IGD result proves Internet reachability.

## Recommended implementation order

1. Correct private global-tracker merging and add the private isolation tests.
2. Replace the SOCKS documentation claim with exact current behavior and add runtime bypass warnings.
3. Introduce a central, injectable `NetworkPolicy`/dialer and route blocklists, torrent URLs, HTTP trackers, and UPnP HTTP through policy-owned clients.
4. Add Proxy-only as a fail-closed mode and prove it with socket-level tests before advertising it as leak-resistant.
5. Expand bind-device/IPv4 enforcement, then add LAN-only destination and redirect filtering.
6. Add router diagnostics and UDP mapping separately; they improve reachability but are not prerequisites for proxy privacy.

Until step 4 passes packet-level tests, the existing SOCKS option should be described as selective routing, not as a privacy mode.
