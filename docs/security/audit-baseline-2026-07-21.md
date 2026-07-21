# Security audit baseline: 2026-07-21

This report records automated dependency findings for commit
`1fd0818e6efc1b48fd15b07fbc09ac8ad6e524cf` before remediation. It is a
triage baseline, not a security guarantee. A lockfile match does not by itself
show that rqbit reaches the affected API.

## Rust dependencies

The scan used `cargo-audit 0.22.2` with RustSec database commit
`b5fc89b8be99e96f79194d8a6f11e9b4143b99f0`, updated 2026-07-17. It examined
733 locked packages and reported seven vulnerable lockfile entries across five
unique advisories.

| Advisory | Locked package | Initial rqbit reachability assessment |
| --- | --- | --- |
| RUSTSEC-2026-0204 | `crossbeam-epoch 0.9.18` | Present through Prometheus metrics. The affected pointer formatting API was not found in rqbit source. |
| RUSTSEC-2021-0041 | `parse_duration 2.1.1` | Used for CLI arguments and environment variables. The denial of service requires control of local process input, not peer traffic. No patched release is listed. |
| RUSTSEC-2026-0194 | `quick-xml 0.38.4` and `0.39.4` | `0.38.4` directly parses UPnP XML received from the LAN and is runtime-reachable when UPnP functionality is used. `0.39.4` is pulled through Tauri/plist and needs separate target-path validation. |
| RUSTSEC-2026-0195 | `quick-xml 0.38.4` and `0.39.4` | Same dependency paths as RUSTSEC-2026-0194. The direct UPnP path processes untrusted XML. |
| RUSTSEC-2026-0185 | `quinn-proto 0.11.14` | Locked through `reqwest`; its activation under rqbit's selected features and targets must be confirmed before assigning runtime severity. |

The scan also emitted informational warnings, including unsoundness advisories
for `anyhow 1.0.102` and `memmap2 0.9.10`. Searches found no rqbit use of the
affected `anyhow::Error::downcast_mut` or `memmap2` range-advice/flush APIs.
These remain upgrade candidates but are not classified as demonstrated rqbit
vulnerabilities by this report.

## JavaScript dependencies

A live `npm audit --json` reported three affected development/build packages:

| Package | Locked version | Finding |
| --- | --- | --- |
| `vite` | `7.3.3` | Direct development dependency with GHSA-v6wh-96g9-6wx3 and GHSA-fx2h-pf6j-xcff. |
| `esbuild` | `0.27.7` | Transitive Vite dependency with GHSA-g7r4-m6w7-qqqr. |
| `js-yaml` | `4.1.1` | Transitive dependency through `vite-plugin-svgr` / `@svgr/core` / `cosmiconfig`, with GHSA-h67p-54hq-rp68 and GHSA-52cp-r559-cp3m. |

`npm audit --omit=dev --json` reported no production dependency findings. The
listed issues therefore affect development or build environments rather than
JavaScript packages shipped as a Node.js runtime in rqbit.

## Remediation policy

Remediation should be derived from a fresh scan and dependency-path check at
implementation time. Runtime-reachable findings take priority. Build-only,
target-specific, or unused affected APIs may be documented with a time-bounded
exception, but the preferred outcome remains upgrading or replacing the
dependency. CI should regenerate machine-readable audit output so this snapshot
does not become the continuing source of truth.
