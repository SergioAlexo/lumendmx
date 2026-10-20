//! The fixture library as Rust sees it: the parts of a fixture definition
//! (`src/lib/fixtures/types.ts`) that resolving a patch needs. The frontend
//! owns the library — bundled JSON plus user imports — and pushes the full
//! set here whenever it changes (`fixtures_set_library`). Fields Rust doesn't
//! use are ignored on the way in.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureDefinition {
    pub id: String,
    pub manufacturer: String,
    pub model: String,
    #[serde(rename = "type", default)]
    pub category: String,
    pub modes: Vec<FixtureMode>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureMode {
    pub name: String,
    pub channel_count: u16,
    pub channels: Vec<FixtureChannel>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureChannel {
    /// 1-based offset within the mode's channel block (the coarse channel
    /// of a 16-bit pair).
    pub offset: u16,
    #[serde(default)]
    pub fine_offset: Option<u16>,
    /// Which repeated cell this channel belongs to (multi-cell fixtures);
    /// absent or 0 for a whole-fixture channel.
    #[serde(default)]
    pub cell: Option<u32>,
    /// The attribute ("dimmer", "red", "pan" ...).
    #[serde(rename = "type")]
    pub attribute: String,
    #[serde(default)]
    pub label: Option<String>,
    /// Value held when nothing drives the channel, e.g. a function channel's
    /// "obey DMX" value so the fixture doesn't run a built-in program.
    #[serde(default)]
    pub default_value: Option<u8>,
    #[serde(default)]
    pub capabilities: Vec<ChannelCapability>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelCapability {
    pub dmx_range: [u8; 2],
    #[serde(rename = "type")]
    pub attribute: String,
    #[serde(default)]
    pub label: Option<String>,
    /// A colour-wheel slot's colour, "#rrggbb".
    #[serde(default)]
    pub color: Option<String>,
}

/// One fixture type as listed to the UI and MCP clients.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct FixtureTypeSummary {
    pub id: String,
    pub manufacturer: String,
    pub model: String,
    pub category: String,
    pub modes: Vec<FixtureModeSummary>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct FixtureModeSummary {
    pub index: u32,
    pub name: String,
    pub channel_count: u16,
    /// Attribute of each channel, in offset order.
    pub attributes: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct FixtureLibrary {
    fixtures: HashMap<String, FixtureDefinition>,
}

impl FixtureLibrary {
    /// Parses each definition on its own, so one malformed import doesn't
    /// drop the whole library. Returns the ids that failed to parse.
    pub fn from_json(defs: Vec<serde_json::Value>) -> (Self, Vec<String>) {
        let mut fixtures = HashMap::new();
        let mut rejected = Vec::new();
        for raw in defs {
            let id = raw.get("id").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            match serde_json::from_value::<FixtureDefinition>(raw) {
                Ok(def) => {
                    fixtures.insert(def.id.clone(), def);
                }
                Err(e) => rejected.push(format!("{id}: {e}")),
            }
        }
        (Self { fixtures }, rejected)
    }

    #[cfg(test)]
    pub fn from_defs(defs: Vec<FixtureDefinition>) -> Self {
        Self { fixtures: defs.into_iter().map(|d| (d.id.clone(), d)).collect() }
    }

    pub fn get(&self, id: &str) -> Option<&FixtureDefinition> {
        self.fixtures.get(id)
    }

    pub fn mode(&self, id: &str, mode_index: u32) -> Option<&FixtureMode> {
        self.get(id)?.modes.get(mode_index as usize)
    }

    pub fn summaries(&self) -> Vec<FixtureTypeSummary> {
        let mut list: Vec<FixtureTypeSummary> = self
            .fixtures
            .values()
            .map(|def| FixtureTypeSummary {
                id: def.id.clone(),
                manufacturer: def.manufacturer.clone(),
                model: def.model.clone(),
                category: def.category.clone(),
                modes: def
                    .modes
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        let mut channels: Vec<&FixtureChannel> = m.channels.iter().collect();
                        channels.sort_by_key(|c| c.offset);
                        FixtureModeSummary {
                            index: i as u32,
                            name: m.name.clone(),
                            channel_count: m.channel_count,
                            attributes: channels.iter().map(|c| c.attribute.clone()).collect(),
                        }
                    })
                    .collect(),
            })
            .collect();
        list.sort_by(|a, b| (&a.manufacturer, &a.model).cmp(&(&b.manufacturer, &b.model)));
        list
    }
}

#[cfg(test)]
pub mod test_fixtures {
    use super::*;

    fn channel(offset: u16, attribute: &str) -> FixtureChannel {
        FixtureChannel {
            offset,
            fine_offset: None,
            cell: None,
            attribute: attribute.to_string(),
            label: None,
            default_value: None,
            capabilities: Vec::new(),
        }
    }

    /// A 6-channel par: dimmer, red, green, blue, strobe, function (default 10).
    pub fn rgb_par() -> FixtureDefinition {
        let mut function = channel(6, "function");
        function.default_value = Some(10);
        FixtureDefinition {
            id: "par".to_string(),
            manufacturer: "Test".to_string(),
            model: "Par".to_string(),
            category: "LED Par".to_string(),
            modes: vec![FixtureMode {
                name: "6ch".to_string(),
                channel_count: 6,
                channels: vec![
                    channel(1, "dimmer"),
                    channel(2, "red"),
                    channel(3, "green"),
                    channel(4, "blue"),
                    channel(5, "strobe"),
                    function,
                ],
            }],
        }
    }

    /// A 2-cell RGB bar: cell 1 on channels 1-3, cell 2 on 4-6.
    pub fn rgb_bar() -> FixtureDefinition {
        let cell = |offset: u16, attribute: &str, cell: u32| FixtureChannel { cell: Some(cell), ..channel(offset, attribute) };
        FixtureDefinition {
            id: "bar".to_string(),
            manufacturer: "Test".to_string(),
            model: "Bar".to_string(),
            category: "LED Bar".to_string(),
            modes: vec![FixtureMode {
                name: "6ch".to_string(),
                channel_count: 6,
                channels: vec![
                    cell(1, "red", 1),
                    cell(2, "green", 1),
                    cell(3, "blue", 1),
                    cell(4, "red", 2),
                    cell(5, "green", 2),
                    cell(6, "blue", 2),
                ],
            }],
        }
    }

    pub fn library() -> FixtureLibrary {
        FixtureLibrary::from_defs(vec![rgb_par(), rgb_bar()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_frontend_json_shape_and_reports_bad_entries() {
        let good = serde_json::json!({
            "id": "p", "manufacturer": "M", "model": "Par", "type": "LED Par", "notes": "ignored",
            "modes": [{ "name": "3ch", "channelCount": 3, "channels": [
                { "offset": 1, "type": "red" },
                { "offset": 2, "type": "pan", "fineOffset": 3,
                  "capabilities": [{ "dmxRange": [0, 255], "type": "pan", "physicalRange": { "min": 0, "max": 540, "unit": "deg" } }] }
            ]}]
        });
        let bad = serde_json::json!({ "id": "broken", "modes": "nope" });
        let (library, rejected) = FixtureLibrary::from_json(vec![good, bad]);
        assert_eq!(library.mode("p", 0).unwrap().channels[1].fine_offset, Some(3));
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].starts_with("broken:"));
    }
}
