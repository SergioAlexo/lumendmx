//! Owns every active universe's output thread. A universe is "active" once
//! `configure` has been called for it (normally: once per `UniverseConfig` in
//! the open show, done on show load/patch-panel edits); an inactive universe
//! id simply isn't in the map and reads/writes to it error out clearly.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use super::config::DriverConfig;
use super::runner::{UniverseOutput, UniverseStatus};
use super::{artnet, enttec_pro, ftdi, null, sacn, OutputDriver, UNIVERSE_SIZE};

struct ActiveUniverse {
    config: DriverConfig,
    output: UniverseOutput,
}

pub struct OutputManager {
    app: AppHandle,
    blackout: Arc<AtomicBool>,
    universes: Mutex<HashMap<u32, ActiveUniverse>>,
}

impl OutputManager {
    /// `blackout` is the engine's flag, so both halves agree on it.
    pub fn new(app: AppHandle, blackout: Arc<AtomicBool>) -> Self {
        Self {
            app,
            blackout,
            universes: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_blackout(&self, active: bool) {
        self.blackout.store(active, Ordering::Relaxed);
    }

    pub fn get_blackout(&self) -> bool {
        self.blackout.load(Ordering::Relaxed)
    }

    /// Creates or replaces the driver for `universe_id`. Replacing stops the
    /// previous driver's thread first (via `UniverseOutput`'s `Drop`).
    pub fn configure(&self, universe_id: u32, config: DriverConfig) -> Result<(), String> {
        let driver = build_driver(&config)?;
        let rate_hz = config.rate_hz();
        let output = UniverseOutput::spawn(
            universe_id,
            driver,
            rate_hz,
            self.blackout.clone(),
            self.app.clone(),
        );
        self.universes
            .lock()
            .unwrap()
            .insert(universe_id, ActiveUniverse { config, output });
        Ok(())
    }

    pub fn remove(&self, universe_id: u32) {
        self.universes.lock().unwrap().remove(&universe_id);
    }

    pub fn set_universe_data(&self, universe_id: u32, data: [u8; UNIVERSE_SIZE]) -> Result<(), String> {
        let universes = self.universes.lock().unwrap();
        let active = universes
            .get(&universe_id)
            .ok_or_else(|| format!("Universe {universe_id} has no driver configured"))?;
        active.output.set_frame(data);
        Ok(())
    }

    pub fn get_universe_data(&self, universe_id: u32) -> Result<[u8; UNIVERSE_SIZE], String> {
        let universes = self.universes.lock().unwrap();
        let active = universes
            .get(&universe_id)
            .ok_or_else(|| format!("Universe {universe_id} has no driver configured"))?;
        Ok(active.output.get_frame())
    }

    pub fn status(&self, universe_id: u32) -> Option<UniverseStatus> {
        self.universes
            .lock()
            .unwrap()
            .get(&universe_id)
            .map(|a| a.output.status())
    }

    pub fn all_statuses(&self) -> Vec<UniverseStatus> {
        self.universes
            .lock()
            .unwrap()
            .values()
            .map(|a| a.output.status())
            .collect()
    }

    /// Ids of every universe with a driver configured (whether or not it's
    /// currently connected) — what the engine tick loop writes merged frames
    /// into each tick.
    pub fn universe_ids(&self) -> Vec<u32> {
        self.universes.lock().unwrap().keys().copied().collect()
    }

    pub fn driver_config(&self, universe_id: u32) -> Option<DriverConfig> {
        self.universes
            .lock()
            .unwrap()
            .get(&universe_id)
            .map(|a| a.config.clone())
    }
}

fn build_driver(config: &DriverConfig) -> Result<Box<dyn OutputDriver>, String> {
    Ok(match config {
        DriverConfig::Null => Box::new(null::NullDriver),
        DriverConfig::Ftdi { port, .. } => Box::new(ftdi::FtdiDriver::new(port.clone())),
        DriverConfig::EnttecPro {
            port,
            universe_index,
            ..
        } => Box::new(enttec_pro::EnttecProDriver::new(port.clone(), *universe_index)),
        DriverConfig::ArtNet {
            destination,
            net,
            subnet,
            universe,
            ..
        } => Box::new(artnet::ArtNetDriver::new(destination, *net, *subnet, *universe)?),
        DriverConfig::Sacn {
            destination,
            universe,
            priority,
            ..
        } => Box::new(sacn::SacnDriver::new(destination, *universe, *priority)?),
    })
}
