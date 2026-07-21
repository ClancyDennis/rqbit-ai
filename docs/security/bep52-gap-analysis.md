# BEP 52 gap analysis

Scope: repository HEAD `1fd0818e6efc1b48fd15b07fbc09ac8ad6e524cf`.

This document describes code that exists at this commit. It does not infer support from type names or planned error variants.
All source locations refer to that immutable baseline; later edits may shift local line numbers.

## Status definitions

- **Implemented and tested**: a runtime-capable implementation has a focused repository test.
- **Implemented**: a runtime-capable implementation exists, but no focused BEP 52 test was found.
- **Scaffold-only**: supporting types, primitives, or error variants exist but are not connected to an end-to-end v2 path.
- **Missing**: the required model or runtime path was not found.

## Capability matrix

| Capability | Status | Evidence and gap |
| --- | --- | --- |
| v2 magnet parsing | **Implemented and tested (parser only)** | `Magnet` stores both `Id20` and `Id32`, parses `xt=urn:btmh:1220...`, and renders an `Id32` back to a magnet (`crates/librqbit_core/src/magnet.rs:7-14`, `crates/librqbit_core/src/magnet.rs:81-98`, `crates/librqbit_core/src/magnet.rs:158-165`). `test_parse_magnet_v2` covers parsing (`crates/librqbit_core/src/magnet.rs:200-212`). This does not make v2-only magnets addable: `Session::add_torrent` immediately requires `as_id20()` and errors if the magnet has no BTv1 hash (`crates/librqbit/src/session.rs:1095-1100`). No hybrid-magnet round-trip or session-level test was found. |
| `Id32` and SHA-256 primitives | **Implemented and tested as primitives; scaffold-only for torrents** | `Id32` is an `Id<32>` with parsing/serde inherited from the generic type and a BEP 52 truncation helper (`crates/librqbit_core/src/hash_id.rs:7-25`, `crates/librqbit_core/src/hash_id.rs:86-113`, `crates/librqbit_core/src/hash_id.rs:179-193`). Tests cover hex parsing and 20-byte truncation (`crates/librqbit_core/src/hash_id.rs:213-217`, `crates/librqbit_core/src/hash_id.rs:228-243`). `sha1w` implements SHA-256 for both backends and tests the empty-string vector (`crates/sha1w/src/lib.rs:14-26`, `crates/sha1w/src/lib.rs:62-85`, `crates/sha1w/src/lib.rs:118-139`, `crates/sha1w/src/lib.rs:155-173`). Repository-wide references show these SHA-256 implementations are not used by torrent parsing or piece verification. `Id32::truncate_for_dht` is likewise only referenced by its unit test. |
| v2 info dictionary | **Missing** | The only metainfo model is named and shaped as v1: `ParsedTorrent` contains `TorrentMetaV1`, and `TorrentMetaV1` stores an `Id20` info hash (`crates/librqbit_core/src/torrent_metainfo.rs:13-22`, `crates/librqbit_core/src/torrent_metainfo.rs:45-72`). `TorrentMetaV1Info` requires the v1 `pieces` string and exposes v1 single-file or `files` layouts; it has no `meta version` or `file tree` fields (`crates/librqbit_core/src/torrent_metainfo.rs:83-115`). `torrent_from_bytes` always SHA-1 hashes the raw info dictionary into `Id20` (`crates/librqbit_core/src/torrent_metainfo.rs:24-38`). |
| v2 file tree parsing and validation | **Scaffold-only** | Errors exist for missing/invalid file trees, meta version, and v2 paths (`crates/librqbit_core/src/error.rs:24-34`), but none is referenced outside its declaration. File iteration and validation operate on v1 `length`/`files` and v1 paths (`crates/librqbit_core/src/torrent_metainfo.rs:279-303`, `crates/librqbit_core/src/torrent_metainfo.rs:306-355`, `crates/librqbit_core/src/torrent_metainfo.rs:387-421`). No v2 file-tree structs, parser, validator, or fixture was found. |
| piece layers and Merkle verification | **Scaffold-only** | There are detailed error variants for piece-layer presence, size, count, roots, and small/zero-length files (`crates/librqbit_core/src/error.rs:35-52`), plus the standalone SHA-256 primitive above. None of those errors is used. Runtime piece verification hashes a flat v1 piece with SHA-1 and compares it against 20-byte entries in `info.pieces` (`crates/librqbit_core/src/torrent_metainfo.rs:358-369`, `crates/librqbit/src/file_ops.rs:112-180`, `crates/librqbit/src/file_ops.rs:186-260`). No Merkle tree, pad-hash, piece-layer parser, or root validation implementation was found. |
| hybrid consistency validation | **Scaffold-only** | `V2HybridFileListMismatch` exists but has no caller (`crates/librqbit_core/src/error.rs:53-56`). Because the active schema only captures the v1 file list, it cannot compare the v1 and v2 file layouts. A hybrid metainfo file may be usable through its v1 fields while unknown v2 fields are not represented, but there is no test or consistency validation; this must not be described as verified hybrid support. |
| v2 metadata exchange | **Missing** | Magnet resolution, peer connections, and the wire handshake are keyed by `Id20` (`crates/librqbit/src/peer_info_reader/mod.rs:33-44`, `crates/librqbit/src/peer_connection.rs:82-89`, `crates/peer_binary_protocol/src/lib.rs:467-486`). Received metadata is SHA-1 checked and deserialized as `TorrentMetaV1Info` (`crates/librqbit/src/peer_info_reader/mod.rs:113-150`, `crates/librqbit/src/peer_info_reader/mod.rs:186-204`). Serving `ut_metadata` sends the session's existing info bytes (`crates/librqbit/src/torrent_state/live/mod.rs:1234-1245`, `crates/librqbit/src/torrent_state/live/mod.rs:1994-2021`). This is a functional v1 metadata path, not a v2 metadata/piece-layers path. |
| persistence | **Missing for v2** | `SerializedTorrent` stores only `Id20`, and magnet restoration always calls `Magnet::from_id20` (`crates/librqbit/src/session_persistence/mod.rs:20-49`). JSON torrent and fast-resume filenames are keyed by `Id20` (`crates/librqbit/src/session_persistence/json.rs:116-122`); the PostgreSQL adapter also reconstructs `Id20` from the stored bytes (`crates/librqbit/src/session_persistence/postgres.rs:23-37`). There is no schema/version strategy for a 32-byte identity, dual hybrid identities, piece layers, or v2 resume state. |
| v2 download and verification | **Missing** | Session metadata, add responses, and torrent state use `TorrentMetaV1Owned`, `ValidatedTorrentMetaV1Info`, and `Id20` (`crates/librqbit/src/session.rs:79-93`, `crates/librqbit/src/session.rs:296-303`, `crates/librqbit/src/torrent_state/mod.rs:132-187`). The download path verifies completed pieces through the SHA-1 `FileOps::check_piece` path (`crates/librqbit/src/torrent_state/live/mod.rs:1918-1976`). A hybrid torrent can at most follow this v1 path; no v2 hashes are verified. |
| v2 resume/recheck | **Missing** | Initial and fast-resume checks call the same v1 SHA-1 `FileOps` verifier, and bitfields are loaded/stored under `TorrentIdOrHash`, whose hash variant is `Id20` (`crates/librqbit/src/torrent_state/initializing.rs:188-225`, `crates/librqbit/src/file_ops.rs:72-184`, `crates/librqbit/src/api.rs:46-50`). There is no v2 file-boundary or Merkle-aware resume validation. |
| v2 seeding | **Missing** | Upload and metadata serving run from the same v1-shaped session metadata and `Id20` peer handshake. Torrent creation also produces only SHA-1 piece hashes, a `TorrentMetaV1Info`, an `Id20`, and a BTIH magnet (`crates/librqbit/src/create_torrent_file.rs:35-48`, `crates/librqbit/src/create_torrent_file.rs:100-178`, `crates/librqbit/src/create_torrent_file.rs:187-203`). There is no v2 torrent creation or v2 swarm identity to seed. Hybrid data may be served to the v1 swarm only. |
| API identifiers | **Missing for v2** | `TorrentIdOrHash::Hash` contains `Id20`; parsing accepts a 40-character hash or numeric ID (`crates/librqbit/src/api.rs:46-60`, `crates/librqbit/src/api.rs:161-169`). API responses receive `&Id20` and expose that string as `info_hash` (`crates/librqbit/src/api.rs:537-552`, `crates/librqbit/src/api.rs:562-600`). There is no 64-hex/multihash identifier, hash-version discriminator, or dual hybrid identity in the API. |
| BEP 52 fixtures and interoperability tests | **Missing** | Repository torrent fixtures are the Ubuntu v1 torrents and a small private v1 torrent (`crates/librqbit/resources/ubuntu-21.04-desktop-amd64.iso.torrent`, `crates/librqbit/resources/ubuntu-21.04-live-server-amd64.iso.torrent`, `crates/librqbit_core/src/resources/test/private.torrent`). The only focused v2 tests found cover magnet parsing, `Id32`, truncation, and the standalone SHA-256 vector. No BEP 52 reference metainfo, piece-layer vectors, hybrid mismatch cases, end-to-end v2 transfer, resume, seeding, or external-client interoperability harness was found. |

## Practical capability statement

At this commit, rqbit does **not** provide an end-to-end BitTorrent v2 implementation. It has useful groundwork:

- parsing and rendering of `btmh:1220` magnet topics;
- a generic 32-byte identifier and truncation helper;
- SHA-256 backend wrappers; and
- planned, typed validation errors.

The operational torrent path remains v1. In particular, a v2-only magnet is parsed by `librqbit_core` but rejected by `Session::add_torrent` because it has no BTv1 hash. A hybrid torrent should be treated as an unverified v1 compatibility case: rqbit may consume its v1 fields, but it neither models nor checks the v2 side.

## Implementation dependency order

The gaps form a dependency chain rather than independent features:

1. Add v2 and hybrid metainfo models, raw-info SHA-256 identity, file-tree traversal, and validation with official/golden malformed fixtures.
2. Implement piece-layer parsing and Merkle verification, then make the storage/checking layer select v1, v2, or both for hybrids.
3. Introduce an explicit torrent identity abstraction that can carry v1, v2, or both; migrate session lookup, tracker/DHT keys, API identifiers, and persistence with backward-compatible decoding.
4. Extend magnet and metadata acquisition so a v2-only topic can obtain all data required to construct validated v2 metadata.
5. Add download, recheck/resume, upload, and torrent-creation support, followed by v2-only and hybrid interoperability tests against named external clients.

Until steps 1-3 are complete, connecting isolated `Id32` or SHA-256 primitives to session code would risk producing ambiguous identities and unverifiable downloads.
