//! MIDI input ports via `midir`: listing and opening, with raw messages
//! handed to a callback on midir's own thread. Phase 8 uses it for MIDI
//! clock and MIDI Time Code; Phase 9's control mapping builds on the same
//! two functions.

use midir::{Ignore, MidiInput, MidiInputConnection};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct MidiPortInfo {
    /// Stable identifier to pass back to `open_input`.
    pub id: String,
    pub name: String,
}

const CLIENT_NAME: &str = "LumenDMX";

pub fn list_input_ports() -> Vec<MidiPortInfo> {
    let Ok(input) = MidiInput::new(CLIENT_NAME) else {
        return Vec::new();
    };
    input
        .ports()
        .iter()
        .map(|port| MidiPortInfo { id: port.id(), name: input.port_name(port).unwrap_or_else(|_| port.id()) })
        .collect()
}

/// An open input port; dropping it closes the port.
pub struct MidiInputHandle {
    _connection: MidiInputConnection<()>,
}

/// Opens `port_id` and calls `on_message(timestamp_us, bytes)` for every
/// message — including clock, active sensing and SysEx, which midir would
/// otherwise filter out.
pub fn open_input(
    port_id: &str,
    mut on_message: impl FnMut(u64, &[u8]) + Send + 'static,
) -> Result<MidiInputHandle, String> {
    let mut input = MidiInput::new(CLIENT_NAME).map_err(|e| format!("MIDI unavailable: {e}"))?;
    input.ignore(Ignore::None);
    let port = input.find_port_by_id(port_id).ok_or_else(|| "That MIDI port is no longer available".to_string())?;
    let connection = input
        .connect(&port, "LumenDMX timing in", move |stamp, bytes, _| on_message(stamp, bytes), ())
        .map_err(|e| format!("Couldn't open the MIDI port: {e}"))?;
    Ok(MidiInputHandle { _connection: connection })
}
