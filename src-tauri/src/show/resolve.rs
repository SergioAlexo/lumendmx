//! Patch → DMX addresses: for every patched fixture, which (universe,
//! channel) each of its attributes lives on, plus what the merge needs to
//! know about the patch as a whole (which channels are HTP intensity, and the
//! values channels rest at when nothing drives them).

use std::collections::{HashMap, HashSet};

use super::library::{ChannelCapability, FixtureChannel, FixtureLibrary};
use crate::engine::state::ChannelKey;
use crate::showfile::schema::PatchedFixture;

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeChannel {
    pub coarse: u16,
    /// The LSB channel of a 16-bit attribute.
    pub fine: Option<u16>,
    pub capabilities: Vec<ChannelCapability>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedFixture {
    pub fixture_number: u32,
    pub universe: u32,
    /// Whole-fixture attributes. For a multi-cell fixture with no
    /// whole-fixture channel for an attribute, its first cell's channel.
    pub attributes: HashMap<String, AttributeChannel>,
    /// Each cell's attributes, in cell order. Empty for a single-cell fixture.
    pub cells: Vec<HashMap<String, AttributeChannel>>,
    pub invert_pan: bool,
    pub invert_tilt: bool,
    pub swap_pan_tilt: bool,
}

impl ResolvedFixture {
    pub fn channel(&self, attribute: &str) -> Option<u16> {
        self.attributes.get(attribute).map(|a| a.coarse)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedPatch {
    /// In patch order.
    pub fixtures: Vec<ResolvedFixture>,
    /// Dimmer channels, per universe: these merge HTP.
    pub htp: HashMap<u32, HashSet<u16>>,
    /// Channel rest values from the fixture library's `defaultValue`s.
    pub defaults: HashMap<ChannelKey, f32>,
    /// Patched fixtures whose type or mode isn't in the library.
    pub unresolved: Vec<u32>,
}

impl ResolvedPatch {
    pub fn fixture(&self, number: u32) -> Option<&ResolvedFixture> {
        self.fixtures.iter().find(|f| f.fixture_number == number)
    }
}

fn attribute_channel(address: u16, ch: &FixtureChannel) -> AttributeChannel {
    AttributeChannel {
        coarse: address + ch.offset - 1,
        fine: ch.fine_offset.map(|o| address + o - 1),
        capabilities: ch.capabilities.clone(),
    }
}

pub fn resolve(patch: &[PatchedFixture], library: &FixtureLibrary) -> ResolvedPatch {
    let mut out = ResolvedPatch::default();
    for instance in patch {
        let Some(mode) = library.mode(&instance.fixture_id, instance.mode_index) else {
            out.unresolved.push(instance.fixture_number);
            continue;
        };
        let mut attributes: HashMap<String, AttributeChannel> = HashMap::new();
        let mut cells: HashMap<u32, HashMap<String, AttributeChannel>> = HashMap::new();
        let mut channels: Vec<&FixtureChannel> = mode.channels.iter().collect();
        channels.sort_by_key(|c| c.offset);
        for ch in channels {
            let resolved = attribute_channel(instance.address, ch);
            let key = (instance.universe, resolved.coarse);
            if ch.attribute == "dimmer" {
                out.htp.entry(instance.universe).or_default().insert(resolved.coarse);
            }
            if let Some(value) = ch.default_value {
                out.defaults.insert(key, value as f32);
            }
            match ch.cell.filter(|&c| c > 0) {
                Some(cell) => {
                    cells.entry(cell).or_default().entry(ch.attribute.clone()).or_insert(resolved);
                }
                // First occurrence of each attribute wins.
                None => {
                    attributes.entry(ch.attribute.clone()).or_insert(resolved);
                }
            }
        }
        let mut cell_numbers: Vec<u32> = cells.keys().copied().collect();
        cell_numbers.sort_unstable();
        let cells: Vec<HashMap<String, AttributeChannel>> =
            cell_numbers.into_iter().map(|n| cells.remove(&n).unwrap_or_default()).collect();
        if let Some(first) = cells.first() {
            for (attribute, channel) in first {
                attributes.entry(attribute.clone()).or_insert_with(|| channel.clone());
            }
        }
        out.fixtures.push(ResolvedFixture {
            fixture_number: instance.fixture_number,
            universe: instance.universe,
            attributes,
            cells,
            invert_pan: instance.invert_pan,
            invert_tilt: instance.invert_tilt,
            swap_pan_tilt: instance.swap_pan_tilt,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::library::test_fixtures::library;

    pub fn patched(number: u32, fixture_id: &str, universe: u32, address: u16) -> PatchedFixture {
        PatchedFixture {
            id: format!("f{number}"),
            fixture_id: fixture_id.to_string(),
            mode_index: 0,
            address,
            name: format!("Fixture {number}"),
            universe,
            fixture_number: number,
            invert_pan: false,
            invert_tilt: false,
            swap_pan_tilt: false,
        }
    }

    #[test]
    fn attributes_land_at_address_plus_offset_with_dimmers_htp_and_defaults() {
        let patch = vec![patched(1, "par", 1, 10), patched(2, "par", 2, 1)];
        let resolved = resolve(&patch, &library());
        let first = resolved.fixture(1).unwrap();
        assert_eq!(first.channel("dimmer"), Some(10));
        assert_eq!(first.channel("blue"), Some(13));
        assert_eq!(resolved.htp[&1], HashSet::from([10]));
        assert_eq!(resolved.htp[&2], HashSet::from([1]));
        assert_eq!(resolved.defaults[&(1, 15)], 10.0); // function channel
        assert_eq!(resolved.defaults.len(), 2);
    }

    #[test]
    fn cells_are_listed_in_order_and_the_first_cell_stands_in_for_the_fixture() {
        let resolved = resolve(&[patched(1, "bar", 1, 100)], &library());
        let bar = resolved.fixture(1).unwrap();
        assert_eq!(bar.cells.len(), 2);
        assert_eq!(bar.cells[1]["red"].coarse, 103);
        assert_eq!(bar.channel("red"), Some(100));
    }

    #[test]
    fn unknown_fixture_types_are_reported_not_resolved() {
        let resolved = resolve(&[patched(7, "missing", 1, 1)], &library());
        assert!(resolved.fixtures.is_empty());
        assert_eq!(resolved.unresolved, vec![7]);
    }
}
