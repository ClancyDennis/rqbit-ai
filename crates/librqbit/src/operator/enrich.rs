//! Offline peer enrichment: map a peer IP to its ASN / owning organization so
//! the operator can spot monitoring/surveillance peers (datacenter/hosting
//! ranges) and other non-residential connections.
//!
//! This is pluggable and strictly offline — it does NOT do network lookups on
//! the connection path. Enrichment is a no-op unless a MaxMind GeoLite2-ASN
//! database is configured, in which case lookups are served from the
//! memory-resident db.

use std::net::IpAddr;
use std::path::Path;

use maxminddb::{Reader, geoip2};

#[derive(Debug, Clone, Default)]
pub struct PeerEnrichment {
    pub asn: Option<u32>,
    pub org: Option<String>,
}

pub trait PeerEnricher: Send + Sync {
    fn enrich(&self, ip: IpAddr) -> PeerEnrichment;
}

/// Used when no ASN database is configured.
pub struct NoEnricher;

impl PeerEnricher for NoEnricher {
    fn enrich(&self, _ip: IpAddr) -> PeerEnrichment {
        PeerEnrichment::default()
    }
}

/// Backed by a MaxMind GeoLite2/GeoIP2 ASN `.mmdb` loaded into memory.
pub struct MaxmindEnricher {
    reader: Reader<Vec<u8>>,
}

impl MaxmindEnricher {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let reader = Reader::open_readfile(path)
            .map_err(|e| anyhow::anyhow!("failed to open MaxMind ASN db: {e}"))?;
        Ok(Self { reader })
    }
}

impl PeerEnricher for MaxmindEnricher {
    fn enrich(&self, ip: IpAddr) -> PeerEnrichment {
        // Not-found / decode errors just mean "no data for this IP".
        match self.reader.lookup(ip).map(|r| r.decode::<geoip2::Asn>()) {
            Ok(Ok(Some(asn))) => PeerEnrichment {
                asn: asn.autonomous_system_number,
                org: asn.autonomous_system_organization.map(|s| s.to_string()),
            },
            _ => PeerEnrichment::default(),
        }
    }
}

/// Build an enricher from config: MaxMind if a db path is set and opens
/// successfully, otherwise a no-op (with a warning on failure).
pub fn build_enricher(asn_db_path: Option<&Path>) -> Box<dyn PeerEnricher> {
    match asn_db_path {
        Some(p) => match MaxmindEnricher::open(p) {
            Ok(e) => {
                tracing::info!(path = %p.display(), "operator: loaded ASN database");
                Box::new(e)
            }
            Err(e) => {
                tracing::warn!("operator: {e:#}; peer ASN enrichment disabled");
                Box::new(NoEnricher)
            }
        },
        None => Box::new(NoEnricher),
    }
}
