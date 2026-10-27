//! Per-universe driver configuration. Part of the show-file schema
//! (`showfile::schema::UniverseConfig.driver`) as well as the runtime config
//! `manager::OutputManager` builds drivers from — the same JSON shape lives on
//! disk and in memory.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum SacnDestination {
    Multicast,
    #[serde(rename_all = "camelCase")]
    Unicast { address: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum DriverConfig {
    /// No real output. Also used as the loopback/test driver (BUILD_PLAN
    /// Phase 2's "a loopback/null driver for tests").
    #[default]
    Null,
    /// The FTDI Open DMX widget this project was built against: no onboard
    /// framing chip, so the host generates the real hardware break itself
    /// (see `output::ftdi`).
    #[serde(rename_all = "camelCase")]
    Ftdi { port: Option<String>, rate_hz: u32 },
    /// An Enttec DMX USB Pro (or Pro Mk2, which exposes two universes over one
    /// connection via `universe_index` 0/1). Unlike the plain FTDI widget,
    /// the Pro's onboard MCU generates DMX timing itself from a framed
    /// message, so no host-side break/MAB is needed.
    #[serde(rename_all = "camelCase")]
    EnttecPro {
        port: Option<String>,
        universe_index: u8,
        rate_hz: u32,
    },
    /// Art-Net 4 (unicast or broadcast). `net`/`subnet`/`universe` together
    /// form the 15-bit Port-Address (net: 7 bits, subnet: 4 bits, universe:
    /// 4 bits).
    #[serde(rename_all = "camelCase")]
    ArtNet {
        /// Destination IP, e.g. "255.255.255.255" for broadcast or a node's
        /// unicast address.
        destination: String,
        net: u8,
        subnet: u8,
        universe: u8,
        rate_hz: u32,
    },
    /// sACN / E1.31.
    #[serde(rename_all = "camelCase")]
    Sacn {
        destination: SacnDestination,
        universe: u16,
        priority: u8,
        rate_hz: u32,
    },
}

impl DriverConfig {
    pub fn rate_hz(&self) -> u32 {
        match self {
            DriverConfig::Null => 30,
            DriverConfig::Ftdi { rate_hz, .. }
            | DriverConfig::EnttecPro { rate_hz, .. }
            | DriverConfig::ArtNet { rate_hz, .. }
            | DriverConfig::Sacn { rate_hz, .. } => *rate_hz,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DriverConfig::Null => "None",
            DriverConfig::Ftdi { .. } => "FTDI Open DMX",
            DriverConfig::EnttecPro { .. } => "Enttec DMX USB Pro",
            DriverConfig::ArtNet { .. } => "Art-Net",
            DriverConfig::Sacn { .. } => "sACN",
        }
    }
}

pub const DEFAULT_SERIAL_RATE_HZ: u32 = 30;
pub const DEFAULT_NETWORK_RATE_HZ: u32 = 40;
