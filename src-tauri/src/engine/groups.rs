//! Groups: a named, persistent set of fixture numbers, recorded from the
//! current selection (BUILD_PLAN Phase 5). The command line's `GROUP N`
//! selection term (parsed since Phase 3, previously always "no such group")
//! resolves against these.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct Group {
    pub id: u32,
    pub name: String,
    /// Kept in the order they were selected when recorded, per BUILD_PLAN's
    /// "keep selection order" — matters for things like an eventual chase
    /// built from the group.
    pub fixture_numbers: Vec<u32>,
}

#[derive(Default)]
pub struct GroupStore {
    groups: HashMap<u32, Group>,
    next_id: u32,
}

impl GroupStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, name: String, fixture_numbers: Vec<u32>) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.groups.insert(id, Group { id, name, fixture_numbers });
        id
    }

    pub fn rename(&mut self, id: u32, name: String) -> Result<(), String> {
        let group = self.groups.get_mut(&id).ok_or_else(|| format!("No group {id}"))?;
        group.name = name;
        Ok(())
    }

    pub fn delete(&mut self, id: u32) {
        self.groups.remove(&id);
    }

    pub fn get(&self, id: u32) -> Option<&Group> {
        self.groups.get(&id)
    }

    /// Replaces every group (loading a show). New ids continue after the
    /// highest loaded one so they never collide with a loaded group.
    pub fn replace_all(&mut self, groups: Vec<Group>) {
        self.next_id = groups.iter().map(|g| g.id).max().unwrap_or(0);
        self.groups = groups.into_iter().map(|g| (g.id, g)).collect();
    }

    pub fn list(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = self.groups.values().cloned().collect();
        groups.sort_by_key(|g| g.id);
        groups
    }

    /// A plain `id -> fixture_numbers` view, what the command line's
    /// selection resolver needs.
    pub fn fixture_numbers_by_id(&self) -> HashMap<u32, Vec<u32>> {
        self.groups.iter().map(|(&id, g)| (id, g.fixture_numbers.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_lists_groups_in_id_order() {
        let mut store = GroupStore::new();
        let b = store.record("Backs".to_string(), vec![5, 6]);
        let a = store.record("Pars".to_string(), vec![1, 2, 3]);

        let listed = store.list();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, b);
        assert_eq!(listed[1].id, a);
        assert_eq!(listed[1].fixture_numbers, vec![1, 2, 3]);
    }

    #[test]
    fn preserves_selection_order_not_sorted_order() {
        let mut store = GroupStore::new();
        let id = store.record("Odd order".to_string(), vec![3, 1, 2]);
        assert_eq!(store.get(id).unwrap().fixture_numbers, vec![3, 1, 2]);
    }

    #[test]
    fn rename_and_delete() {
        let mut store = GroupStore::new();
        let id = store.record("Old name".to_string(), vec![1]);
        store.rename(id, "New name".to_string()).unwrap();
        assert_eq!(store.get(id).unwrap().name, "New name");

        store.delete(id);
        assert!(store.get(id).is_none());
    }

    #[test]
    fn rename_unknown_group_errors() {
        let mut store = GroupStore::new();
        assert!(store.rename(999, "X".to_string()).is_err());
    }

    #[test]
    fn fixture_numbers_by_id_matches_records() {
        let mut store = GroupStore::new();
        let id = store.record("G".to_string(), vec![7, 8]);
        let map = store.fixture_numbers_by_id();
        assert_eq!(map.get(&id), Some(&vec![7, 8]));
    }
}
