use crate::{
    utils::{target_from_hash_rate, Id},
    Error,
};
use mining_sv2::{Extranonce, OpenStandardMiningChannelSuccess};
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

    /// `downstream_hr` [H/s] is the expected hash rate of the device (or cumulative hashrate on
    /// the channel if multiple devices are connected downstream). Depending on server’s target
    /// setting policy, this value can be used for setting a reasonable target for the channel.
    /// Proxy MUST send 0.0f when there are no mining devices connected yet.
    pub fn new_standard_channel(
        &mut self,
        request_id: u32,
        downstream_hr: f32,
        group_id: u32,
    ) -> Result<OpenStandardMiningChannelSuccess, Error> {
        // Return error if self.groups hashmap is empty
        let channel_id = match self.groups.get_mut(&group_id) {
            Some(cid) => cid.next(),
            None => return Err(Error::NoGroupsFound),
        };

        Ok(OpenStandardMiningChannelSuccess {
            request_id: request_id.into(),
            channel_id,
            target: target_from_hash_rate(downstream_hr, 1.0),
            extranonce_prefix: self.extranonces.next(),
            group_channel_id: group_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binary_sv2::B032;
    use std::convert::TryInto;

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

        let actual_value = (&groups_hashmap)
            .get(&1)
            .expect("Hashmap should not be empty");
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

        let actual_value = (&groups_hashmap)
            .get(&1)
            .expect("Hashmap should not be empty");
        let expect_value = Id::new();
        assert_eq!(expect_value, *actual_value);

        assert!(&groups_hashmap.contains_key(&2));
        let actual_value = (&groups_hashmap)
            .get(&2)
            .expect("Hashmap should not be empty");
        assert_eq!(expect_value, *actual_value);
    }

    #[test]
    fn adds_a_new_standard_channel_to_upstream_with_groups() -> Result<(), Error> {
        // Construct new UpstreamWithGroups
        let mut upstream_with_groups = UpstreamWithGroups::new();
        // Must add a new group channel to the UpstreamWithGroups instance
        let group_channel_id = upstream_with_groups.new_group_channel();
        assert_eq!(1, group_channel_id);

        let request_id = 0;
        let downstream_hr = 1_000_000_000_000.0; // 1 TH/s
        let actual = upstream_with_groups.new_standard_channel(
            request_id,
            downstream_hr,
            group_channel_id,
        )?;

        let new_extranonce: B032 = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ]
        .try_into()
        .expect("Invalid `B032`");
        let mut extranonce = Extranonce::from(new_extranonce);

        let expect = OpenStandardMiningChannelSuccess {
            request_id: request_id.into(),
            channel_id: 1,
            target: target_from_hash_rate(downstream_hr, 1.0),
            extranonce_prefix: extranonce.next(),
            group_channel_id,
        };

        assert_eq!(expect, actual);

        Ok(())
    }

    #[test]
    fn errors_on_a_new_standard_channel_to_upstream_with_groups_if_groups_hashmap_is_empty(
    ) -> Result<(), Error> {
        let expect =
            "A channel was attempted to be added to an Upstream, but no groups are specified";

        let mut upstream_with_groups = UpstreamWithGroups {
            groups: HashMap::new(),
            ids: Id::new(),
            extranonces: Extranonce::new(),
        };

        let group_id = 0;
        let request_id = 0;
        let downstream_hr = 1_000_000_000_000.0; // 1 TH/s

        let err = upstream_with_groups
            .new_standard_channel(request_id, downstream_hr, group_id)
            .unwrap_err();

        assert_eq!(expect, err.to_string());

        Ok(())
    }
}
