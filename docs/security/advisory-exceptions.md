# Dependency advisory exceptions

Exceptions in `.cargo/audit.toml` are temporary risk acceptances, not claims
that the affected package is generally safe. Each exception must name its
dependency path, reachability assessment, owner, review triggers, and expiry.

## RUSTSEC-2026-0194 and RUSTSEC-2026-0195

- **Affected lockfile package:** `quick-xml 0.39.4`
- **Dependency path:** `quick-xml 0.39.4` is a direct dependency only of
  `plist 1.9.0`; `plist` is present only through Tauri packages used by the
  desktop application. rqbit's UPnP parser uses the separately locked,
  patched `quick-xml 0.41.0`.
- **Assessment:** RUSTSEC-2026-0194 affects iteration of start-tag attributes
  with duplicate-name checks enabled. `plist 1.9.0`'s XML reader does not
  iterate attributes or call `try_get_attribute`; it examines element names
  and content. RUSTSEC-2026-0195 affects `NsReader` namespace processing, while
  `plist 1.9.0` imports and constructs the plain `quick_xml::Reader`. The Tauri
  call sites consume local application `Info.plist` files and generated plist
  values, not peer, tracker, DHT, UPnP, or HTTP API input. The affected APIs
  are therefore not reachable through rqbit's untrusted network inputs on the
  documented path.
- **Tool limitation:** `cargo-audit` can ignore only an advisory ID globally;
  it cannot limit an ignore to one package version or dependency path. The
  supply-chain workflow compensates by failing unless the complete locked
  `quick-xml` inventory is exactly `0.39.4` and patched `0.41.0`,
  `quick-xml 0.39.4` has exactly one direct dependent (`plist`), and all direct
  `plist` dependents are Tauri packages. Any added, removed, or changed
  `quick-xml` version requires explicit review, so the advisory-wide ignore
  cannot silently cover another affected version or dependency path.
  `cargo-audit`'s JSON report records the ignored IDs under `settings.ignore`
  but omits their advisory bodies and affected packages. Reviewers should
  therefore read this assessment together with the uploaded
  `quick-xml-inventory.txt`, `quick-xml-0.39.4-inverse-tree.txt`,
  `plist-inverse-tree.txt`, and `quick-xml-exception-status.json` reports.
  Other RustSec advisories and warnings remain visible in `cargo-audit.json`.
- **Owner:** rqbit maintainers.
- **Review triggers:** remove or reassess the exception when Tauri or `plist`
  changes, `plist` adopts a patched `quick-xml`, rqbit begins using `plist`
  directly, the dependency-path check fails, the affected APIs become
  reachable, or new exploitability information is published.
- **Expiry:** 2026-10-21. The exception must be removed, renewed with fresh
  evidence, or replaced by an upstream dependency fix by this date. The
  supply-chain workflow rejects the exception on and after this date.
