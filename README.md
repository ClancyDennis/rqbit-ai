# rqbit-ai

An AI-augmented, security-hardened fork of [**rqbit**](https://github.com/ikatson/rqbit) — a
BitTorrent client in Rust with an HTTP API, Web UI, and a [Tauri](https://tauri.app/) desktop app.

## What this fork adds

- **AI operator** — an optional, in-process supervisory loop that reads client state (~once a
  minute) and takes safe, human-gated actions to improve performance, security, and reliability.
  Works with any OpenAI-compatible endpoint, including a local Ollama model (keeps peer/torrent data
  on-device). See [AI operator](#ai-operator-experimental).
- **Security hardening** — constant-time HTTP Basic Auth, bounded tracker/UPnP/SOAP/torrent-URL
  responses, private-torrent tracker isolation, a tightened Tauri CSP, and a supply-chain CI gate
  (cargo-audit / cargo-deny) with pinned toolchains. See [`SECURITY.md`](SECURITY.md) and
  [`docs/security/`](docs/security).

Everything else is upstream rqbit functionality. This fork is not published to crates.io / Homebrew /
Docker — [build from source](#build) to get the features above. Full credit to the original project.

## Usage quick start

### Optional - start the server

Assuming you are downloading to ~/Downloads.

    rqbit server start ~/Downloads

### Download torrents

Assuming you are downloading to ~/Downloads. By default it'll download to current directory.

    rqbit download [-o ~/Downloads] 'magnet:?....' [https?://url/to/.torrent] [/path/to/local/file.torrent]

## Web UI

Access at http://localhost:3030/web/. See screenshot below (torrent names and speeds are simulated).

<img width="1000" src="https://github.com/user-attachments/assets/d916b3d9-ebbd-462a-889d-df3916cc2681" />

## Desktop app

The desktop app is a [thin wrapper](https://github.com/ikatson/rqbit/blob/main/desktop/src-tauri/src/main.rs) on top of the Web UI frontend.

Download it in [Releases](https://github.com/ikatson/rqbit/releases) for OSX and Windows. For Linux, build manually with

    cargo tauri build

It looks similar to the Web UI (screenshot above).

## Streaming support

rqbit can stream torrent files and smartly block the stream until the pieces are available. The pieces getting streamed are prioritized. All of this allows you to seek and live stream videos for example.

You can also stream to e.g. VLC or other players with HTTP URLs. Supports seeking too (through various range headers).
The streaming URLs look like http://IP:3030/torrents/<torrent_id>/stream/<file_id>

## Integrated UPnP Media Server

rqbit can advertise managed torrents to LAN, e.g. your TVs and stream torrents there (without transcoding). Seeking to arbitrary points in the videos is supported too.

Usage from CLI

```
rqbit --enable-upnp-server server start ...
```

## mDNS advertising

rqbit can advertise its HTTP API on your LAN via mDNS/DNS-SD, so you can open the Web UI at http://rqbit.local:3030/web/ from any device without knowing the server's IP.

Usage from CLI (requires a non-loopback listen address):

```
rqbit --enable-mdns --http-api-listen-addr 0.0.0.0:3030 server start ...
```

## IPv6

rqbit supports IPv6. By default it listens on all interfaces in dualstack mode. It can work even if there's no IPv6 enabled.

## Shell completions

Assuming bash, add this to your `~/.bashrc`. Modify for your shell of choice.

```
eval "$(rqbit completions bash)"
```

## Socks proxy support

```
rqbit --socks-url socks5://[username:password]@host:port ...
```

## Watching a directory for .torrents

```
rqbit server start --watch-folder [path] /download/path
```

## AI operator (experimental)

rqbit can run an optional in-process "AI operator": a supervisory loop that
periodically reads session state, asks an LLM what a vigilant human operator
would do, and (in later versions) applies a gated set of safe, reversible
actions to improve Performance, Security and Reliability. It never participates
in the data plane (piece selection, choke/unchoke, and rate limiting stay
deterministic).

It is compiled out unless you build with the `operator` feature:

```
cargo build --release --features "webui,operator"
```

Then enable it at runtime (it is **off** and **dry-run** by default):

```
rqbit server start \
  --operator-enabled \
  --operator-base-url http://localhost:8080 \
  --operator-model gpt-5.6-luna \
  /download/path
```

Flags (all also settable via `RQBIT_OPERATOR_*` env vars):

- `--operator-enabled` — master on/off switch (off by default).
- `--operator-live` — allow the operator to actually apply actions. Without it
  the operator runs in **dry-run** mode: it logs what it would do but changes
  nothing. Destructive actions (delete/forget) can never be applied
  automatically regardless of this flag.
- `--operator-base-url` — any OpenAI-compatible `/v1/chat/completions` endpoint.
  If unset, the operator runs with a no-op model and makes no suggestions.
- `--operator-model` — the model id, e.g. `gpt-5.6-luna`.
- `--operator-poll-interval-secs` — how often to run (default 45s).
- `--operator-asn-db` — optional path to a MaxMind GeoLite2-ASN `.mmdb`. When set,
  each peer in the snapshot is enriched with its ASN and owning organization,
  letting the operator spot hosting/monitoring-range peers. Purely offline; no
  lookups happen on the connection path.
- `RQBIT_OPERATOR_API_KEY` — bearer token for the endpoint (env only, never a
  flag and never stored). Falls back to `OPENAI_API_KEY` if unset. rqbit (and
  the desktop app) load a `.env` from the working directory at startup, so you
  can keep the key in a file instead of exporting it.

Actions the operator can currently take (all tier-gated as above):
- Auto (may run automatically, subject to cooldowns): pause/resume, global and
  per-torrent up/down rate limits, force-reannounce.
- Notify (surfaced only): file selection.
- Confirm (queued for explicit human approval): ban-peer, forget, delete-with-files.
- Not yet wired to the engine (recognized/gated but no-op): recheck-files,
  add-tracker.

Safety model:

- In **dry-run** (the default) the operator only logs what it would do.
- With `--operator-live` it executes only **reversible, low-risk** actions
  automatically (pause/resume a torrent, adjust global rate limits), capped at a
  few per cycle.
- **Destructive actions (delete/forget) are never executed automatically** — they
  require explicit human confirmation regardless of `--operator-live`. The risk
  tier of every action is decided by rqbit, not by the model, so a
  misbehaving/hostile model cannot escalate a delete into an automatic action.
- Untrusted torrent/peer text is passed to the model strictly as data, never as
  instructions.

### Configuring and monitoring from the Web UI

The web UI (and the Tauri desktop app) has an operator panel (robot icon in the
header) with three sections:

- **Settings** — enable/disable, toggle live vs dry-run, set the endpoint URL,
  model, poll interval and ASN db path. Saved settings persist and take effect
  on the next restart; the API key is **not** editable here (it stays in
  `RQBIT_OPERATOR_API_KEY`).
- **Pending confirmations** — approve/reject destructive or human-gated actions
  (delete, forget, ban-peer) the operator has proposed.
- **Recent decisions** — a live feed of what the operator decided and the
  outcome.

The same data is available over the HTTP API under `/operator/*`.

## Systemd socket activation

rqbit can be started on-demand via [systemd socket activation](https://0pointer.de/blog/projects/socket-activation.html) by installing the [service and socket systemd units](systemd) into `$XDG_CONFIG_HOME/systemd/user/` (`~/.config/systemd/user`) and customizing them to your needs. If the associated [`rqbit.conf`](systemd/rqbit.conf) file is installed in `$XDG_CONFIG_HOME/rqbit/rqbit.conf` (`~/.config/rqbit/rqbit.conf`), it will be used to configure `rqbit` when started via the provided systemd unit.

## Performance

Anecdotally from a few reports, rqbit is faster than other clients they've tried, at least with their default settings.

Memory usage for the server is usually within a few tens of megabytes, which makes it great for e.g. RaspberryPI.

I've got a report that rqbit can saturate a 20Gbps link, although I don't have the hardware to confirm.

## Installation

> **This fork is not packaged.** The published artifacts below are the **original
> upstream rqbit** and do **not** include this fork's AI operator or security
> hardening. To use this fork, [build from source](#build).

Upstream rqbit (original project) is available as:

- Pre-built binaries — upstream [Releases](https://github.com/ikatson/rqbit/releases)
- Homebrew — `brew install rqbit`
- Cargo — `cargo install rqbit`
- Docker — [ikatson/rqbit](https://hub.docker.com/r/ikatson/rqbit)

## Build

A regular Rust build. To include the Web UI and the AI operator (requires `npm` for the Web UI):

    cargo build --release --features "webui,operator"

Plain `cargo build --release` also works but omits the Web UI and operator.

The "webui" feature requires npm installed.

## Some useful options

Run ```rqbit --help``` to see all available CLI options.

### -v <log-level>

Increase verbosity. Possible values: trace, debug, info, warn, error.

### --list

Will print the contents of the torrent file or the magnet link.

### --overwrite

If you want to resume downloading a file that already exists, you'll need to add this option.

### -r / --filename-re

Use a regex here to select files by their names.

## Features (not exhaustive)

### Supported BEPs

- [BEP-3: The BitTorrent Protocol Specification](https://www.bittorrent.org/beps/bep_0003.html)
- [BEP-5: DHT Protocol](https://www.bittorrent.org/beps/bep_0005.html)
- [BEP-7: IPv6 Tracker Extension](https://www.bittorrent.org/beps/bep_0007.html)
- [BEP-9: Extension for Peers to Send Metadata Files](https://www.bittorrent.org/beps/bep_0009.html)
- [BEP-10: Extension Protocol](https://www.bittorrent.org/beps/bep_0010.html)
- [BEP-11: Peer Exchange (PEX)](https://www.bittorrent.org/beps/bep_0011.html)
- [BEP-12: Multitracker Metadata Extension](https://www.bittorrent.org/beps/bep_0012.html)
- [BEP-14: Local service discovery](https://www.bittorrent.org/beps/bep_0014.html)
- [BEP-15: UDP Tracker Protocol](https://www.bittorrent.org/beps/bep_0015.html)
- [BEP-20: Peer ID Conventions](https://www.bittorrent.org/beps/bep_0020.html)
- [BEP-23: Tracker Returns Compact Peer Lists](https://www.bittorrent.org/beps/bep_0023.html)
- [BEP-27: Private Torrents](https://www.bittorrent.org/beps/bep_0027.html)
- [BEP-29: uTorrent Transport Protocol](https://www.bittorrent.org/beps/bep_0029.html)
- [BEP-32: IPv6 extension for DHT](https://www.bittorrent.org/beps/bep_0032.html)
- [BEP-47: Padding files and extended file attributes](https://www.bittorrent.org/beps/bep_0047.html)
- [BEP-53: Magnet URI extension - Select specific file indices for download](https://www.bittorrent.org/beps/bep_0053.html)

### Some supported features

- Sequential downloading (the default and only option)
- Resume downloading file(s) if they already exist on disk
- Selective downloading using a regular expression for filename
- DHT support. Allows magnet links to work, and makes more peers available.
- HTTP API
- Pausing / unpausing / deleting (with files or not) APIs
- Stateful server
- Web UI
- Streaming, with seeking
- UPNP port forwarding to your router
- UPNP Media Server
- mDNS advertising
- Fastresume (no rehashing)
- Download / upload rate limiting
- Prometheus metrics at ```/metrics``` and ```/torrents/<id_or_infohash>/peer_stats/prometheus```

## HTTP API

By default it listens on http://127.0.0.1:3030.

```
curl -s 'http://127.0.0.1:3030/'

{
  "apis": {
    "GET /": "list all available APIs",
    "GET /dht/stats": "DHT stats",
    "GET /dht/table": "DHT routing table",
    "GET /metrics": "Prometheus metrics",
    "GET /stats": "Global session stats",
    "GET /stream_logs": "Continuously stream logs",
    "GET /torrents": "List torrents",
    "GET /torrents/playlist": "Playlist for supported players",
    "GET /torrents/{id_or_infohash}": "Torrent details",
    "GET /torrents/{id_or_infohash}/haves": "The bitfield of have pieces",
    "GET /torrents/{id_or_infohash}/metadata": "Download the corresponding torrent file",
    "GET /torrents/{id_or_infohash}/peer_stats": "Per peer stats",
    "GET /torrents/{id_or_infohash}/peer_stats/prometheus": "Per peer stats in prometheus format",
    "GET /torrents/{id_or_infohash}/playlist": "Playlist for supported players",
    "GET /torrents/{id_or_infohash}/stats/v1": "Torrent stats",
    "GET /torrents/{id_or_infohash}/stream/{file_idx}": "Stream a file. Accepts Range header to seek.",
    "GET /web/": "Web UI",
    "POST /rust_log": "Set RUST_LOG to this post launch (for debugging)",
    "POST /torrents": "Add a torrent here. magnet: or http:// or a local file.",
    "POST /torrents/create": "Create a torrent and start seeding. Body should be a local folder",
    "POST /torrents/resolve_magnet": "Resolve a magnet to torrent file bytes",
    "POST /torrents/{id_or_infohash}/add_peers": "Add peers (newline-delimited)",
    "POST /torrents/{id_or_infohash}/delete": "Forget about the torrent, remove the files",
    "POST /torrents/{id_or_infohash}/forget": "Forget about the torrent, keep the files",
    "POST /torrents/{id_or_infohash}/pause": "Pause torrent",
    "POST /torrents/{id_or_infohash}/start": "Resume torrent",
    "POST /torrents/{id_or_infohash}/update_only_files": "Change the selection of files to download. You need to POST json of the following form {\"only_files\": [0, 1, 2]}"
  },
  "server": "rqbit",
  "version": "9.0.0-beta.1"
}
```

### Basic auth

For HTTP API basic authentication set RQBIT_HTTP_BASIC_AUTH_USERPASS environment variable.

```
RQBIT_HTTP_BASIC_AUTH_USERPASS=username:password rqbit server start ...
```

### Add torrent through HTTP API

`curl -d 'magnet:?...' http://127.0.0.1:3030/torrents`

OR

`curl -d 'http://.../file.torrent' http://127.0.0.1:3030/torrents`

OR

`curl --data-binary @/tmp/xubuntu-23.04-minimal-amd64.iso.torrent http://127.0.0.1:3030/torrents`

Supported query parameters, all optional:

- overwrite=true|false
- only_files_regex - the regular expression string to match filenames
- output_folder - the folder to download to. If not specified, defaults to the one that rqbit server started with
- list_only=true|false - if you want to just list the files in the torrent instead of downloading

## Code organization

- crates/rqbit - main binary
- crates/librqbit - main library
- crates/librqbit-core - torrent utils
- crates/bencode - bencode serializing/deserializing
- crates/buffers - wrappers around binary buffers
- crates/clone_to_owned - a trait to make something owned
- crates/sha1w - wrappers around sha1 libraries
- crates/peer_binary_protocol - the protocol to talk to peers
- crates/dht - Distributed Hash Table implementation
- crates/upnp - upnp port forwarding
- crates/upnp_serve - upnp MediaServer
- desktop - desktop app built with [Tauri](https://tauri.app/)
- [librqbit-utp](https://github.com/ikatson/librqbit-utp/) - uTP protocol
- [librqbit-dualstack-sockets](https://github.com/ikatson/librqbit-dualstack-sockets) - cross-platform IPv6+IPv4 listeners with canonical IPs

## Motivation

BitTorrent is great: fast, distributed, and it works with basically anything. It's also a privacy nightmare, a security crapshoot, and pretty brittle — leaky peer and tracker traffic, weak defaults, and torrents that quietly stall or misbehave.

So, in the post-ChatGPT era, how do you fix that? With AI. The operator watches the client the way an attentive human would and pursues three goals: improve **security**, improve **reliability**, and improve **performance** — while never touching the data plane (piece selection, choking, and the rate limiter stay deterministic).

rqbit was the clear starting point: fast, open source, and written in Rust. The [original project](https://github.com/ikatson/rqbit) began as the upstream author's, from the bencode protocol on up; this fork adds the AI operator and the security hardening.

## Supporting the original project

This fork stands entirely on [ikatson/rqbit](https://github.com/ikatson/rqbit). If you find rqbit useful, please support the original author:

- [GitHub Sponsors](https://github.com/sponsors/ikatson)
- Crypto
  - ETH (Ethereum) 0x68c54b26b5372d5f091b6c08cc62883686c63527
  - XMR (Monero) 49LcgFreJuedrP8FgnUVB8GkAyoPX7A9PjWfKZA1hNYz5vPCEcYQ9HzKr3pccGR6Lc3V3hn52bukwZShLDhZsk57V41c2ea
  - XNO (Nano) nano_1ghid3z6x41x8cuoffb6bbrt4e14wsqdbyqwp5d8rk166meo3h77q7mkjusr
