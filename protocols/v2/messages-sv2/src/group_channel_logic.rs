use crate::utils::Id;
use mining_sv2::{target_from_hr, Extranonce, OpenStandardMiningChannelSuccess};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct UpstreamWithGroups {
    groups: HashMap<u32, Id>,
    ids: Id,
    extranonces: Extranonce,
}

impl UpstreamWithGroups {
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
            ids: Id::new(),
            extranonces: Extranonce::new(),
        }
    }

    pub fn new_group_channel(&mut self) -> u32 {
        let g_id = self.ids.next();
        self.groups.insert(g_id, Id::new());
        g_id
    }

    pub fn new_standard_channel(
        &mut self,
        request_id: u32,
        downstream_hr: f32,
        group_id: u32,
    ) -> OpenStandardMiningChannelSuccess {
        OpenStandardMiningChannelSuccess {
            request_id,
            channel_id: self.groups.get_mut(&group_id).unwrap().next(),
            target: target_from_hr(downstream_hr),
            extranonce_prefix: self.extranonces.next(),
            group_channel_id: group_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_upstream_with_groups() {
        let expect = UpstreamWithGroups {
            groups: HashMap::new(),
            ids: Id::new(),
            extranonces: Extranonce::new(),
        };

        let actual = UpstreamWithGroups::new();

        assert!(actual.groups.is_empty());
        assert_eq!(expect.ids, actual.ids);
        assert_eq!(expect.extranonces, actual.extranonces);
    }

    #[test]
    fn adds_a_new_group_channel_to_upstream_with_groups() {
        let mut expect_upstream_with_groups = UpstreamWithGroups {
            groups: HashMap::new(),
            ids: Id::new(),
            extranonces: Extranonce::new(),
        };

        let mut upstream_with_groups = UpstreamWithGroups {
            groups: HashMap::new(),
            ids: Id::new(),
            extranonces: Extranonce::new(),
        };
        assert!(upstream_with_groups.groups.is_empty());

        // Call `new_group_channel` for the first time on the `UpstreamWithGroups` instance
        let actual = &upstream_with_groups.new_group_channel();

        // Return of `new_group_channel` is the latest group channel id, which is the last group
        // channel id incremented by 1
        assert_eq!(&1, actual);

        // Assert that the `extranonces` field does not change
        assert_eq!(
            expect_upstream_with_groups.extranonces,
            upstream_with_groups.extranonces
        );

        // Increments the `Id` state from 0 to 1
        expect_upstream_with_groups.ids.next();
        assert_eq!(expect_upstream_with_groups.ids, upstream_with_groups.ids);

        // Populates previously empty `groups` hashmap
        assert!(!upstream_with_groups.groups.is_empty());

        // Verify that the group hashmap is 1) populated with a length of 1, 2) the key is `1`, and
        // 3) the value is an Id with a state of 0
        let groups_hashmap = &upstream_with_groups.groups;

        assert_eq!(1, groups_hashmap.len());
        assert!(&groups_hashmap.contains_key(&1));

        let actual_value = (&groups_hashmap).get(&1).unwrap();
        let expect_value = Id::new();
        assert_eq!(expect_value, *actual_value);

        // Call `new_group_channel` for the second time on the `UpstreamWithGroups` instance
        let actual = &upstream_with_groups.new_group_channel();

        // Return of `new_group_channel` is the latest group channel id, which is the last group
        // channel id incremented by 1
        assert_eq!(&2, actual);

        // Assert that the `extranonces` field does not change
        assert_eq!(
            expect_upstream_with_groups.extranonces,
            upstream_with_groups.extranonces
        );

        // Increments the `Id` state from 1 to 2
        expect_upstream_with_groups.ids.next();
        assert_eq!(expect_upstream_with_groups.ids, upstream_with_groups.ids);

        // Verify that the group hashmap is 1) populated with a length of 2, 2) the key is `1`, and
        // 3) the value is an Id with a state of 0
        let groups_hashmap = &upstream_with_groups.groups;

        assert_eq!(2, groups_hashmap.len());
        assert!(&groups_hashmap.contains_key(&1));

        let actual_value = (&groups_hashmap).get(&1).unwrap();
        let expect_value = Id::new();
        assert_eq!(expect_value, *actual_value);

        assert!(&groups_hashmap.contains_key(&2));
        let actual_value = (&groups_hashmap).get(&2).unwrap();
        assert_eq!(expect_value, *actual_value);
    }
}
