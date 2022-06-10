use cosmwasm_std::{Addr, BlockInfo, CosmosMsg, Decimal, Timestamp, Uint128};
use cw3::{Status, Vote};
use cw_utils::{Duration, Expiration};
use osmo_bindings::OsmosisMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::helpers::duration_to_expiry;
use crate::threshold::Threshold;

// we multiply by this when calculating needed_votes in order to round up properly
// Note: `10u128.pow(9)` fails as "u128::pow` is not yet stable as a const fn"
const PRECISION_FACTOR: u128 = 1_000_000_000;

// weight of votes for each option
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
pub struct Votes {
    pub yes: Uint128,
    pub no: Uint128,
    pub abstain: Uint128,
    pub veto: Uint128,
}

impl Votes {
    /// sum of all votes
    pub fn total(&self) -> Uint128 {
        self.yes + self.no + self.abstain + self.veto
    }

    /// create it with a yes vote for this much
    pub fn new(init_weight: Uint128) -> Self {
        Votes {
            yes: init_weight,
            no: Uint128::zero(),
            abstain: Uint128::zero(),
            veto: Uint128::zero(),
        }
    }

    pub fn submit(&mut self, vote: Vote, weight: Uint128) {
        match vote {
            Vote::Yes => self.yes = self.yes.checked_add(weight).unwrap(),
            Vote::Abstain => self.abstain = self.abstain.checked_add(weight).unwrap(),
            Vote::No => self.no = self.no.checked_add(weight).unwrap(),
            Vote::Veto => self.veto = self.veto.checked_add(weight).unwrap(),
        }
    }

    pub fn revoke(&mut self, vote: Vote, weight: Uint128) {
        match vote {
            Vote::Yes => self.yes = self.yes.checked_sub(weight).unwrap(),
            Vote::No => self.no = self.no.checked_sub(weight).unwrap(),
            Vote::Abstain => self.abstain = self.abstain.checked_sub(weight).unwrap(),
            Vote::Veto => self.veto = self.veto.checked_sub(weight).unwrap(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
pub struct BlockTime {
    pub height: u64,
    pub time: Timestamp,
}

impl Into<BlockTime> for BlockInfo {
    fn into(self) -> BlockTime {
        BlockTime {
            height: self.height.clone(),
            time: self.time.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Proposal {
    /// Proposal title
    pub title: String,
    /// Related link about this proposal
    pub link: String,
    /// Proposal Description
    pub description: String,
    /// Address of proposer
    pub proposer: Addr,
    /// Current status of this proposal
    pub status: Status,
    /// List of messages to execute
    pub msgs: Vec<CosmosMsg<OsmosisMsg>>,

    /// Starting time / height information
    pub submitted_at: BlockTime,
    pub deposit_ends_at: Expiration,
    pub vote_starts_at: BlockTime,
    pub vote_ends_at: Expiration,

    /// Pass requirements
    pub threshold: Threshold,
    /// The total weight when the proposal started (used to calculate percentages)
    pub total_weight: Uint128,
    /// summary of existing votes
    pub votes: Votes,
    /// Amount of the native governance token required for voting
    pub total_deposit: Uint128,
    pub deposit_base_amount: Uint128,
}

impl Default for Proposal {
    fn default() -> Self {
        Self {
            title: "".to_string(),
            link: "".to_string(),
            description: "".to_string(),
            proposer: Addr::unchecked(""),
            status: Status::Pending,
            msgs: vec![],
            submitted_at: Default::default(),
            deposit_ends_at: Default::default(),
            vote_starts_at: Default::default(),
            vote_ends_at: Default::default(),
            threshold: Default::default(),
            total_weight: Default::default(),
            votes: Default::default(),
            total_deposit: Default::default(),
            deposit_base_amount: Default::default(),
        }
    }
}

impl Proposal {
    pub fn activate_voting_period(&mut self, block_time: BlockTime, voting_period: &Duration) {
        self.status = Status::Open;
        self.vote_starts_at = block_time;
        self.vote_ends_at = duration_to_expiry(&self.vote_starts_at, voting_period);
    }

    /// current_status is non-mutable and returns what the status should be.
    /// (designed for queries)
    pub fn current_status(&self, block: &BlockInfo) -> Status {
        let mut status = self.status;

        match status {
            // if pending, check if voting is opened or timed out
            Status::Pending => {
                // check total deposit amount exceeds deposit base amount
                if self.deposit_base_amount < self.total_deposit {
                    status = Status::Open;
                } else if self.deposit_ends_at.is_expired(block) {
                    // if not and deposit period ended, change proposal status to rejected.
                    status = Status::Rejected;
                }
            }

            // if open, check if voting is passed or timed out
            Status::Open => {
                // check voting period has ended
                if self.vote_ends_at.is_expired(block) {
                    if self.is_passed(block) {
                        status = Status::Passed;
                    } else {
                        status = Status::Rejected;
                    }
                }
            }
            _ => {} // do nothing
        }

        status
    }

    /// update_status sets the status of the proposal to current_status.
    /// (designed for handler logic)
    pub fn update_status(&mut self, block: &BlockInfo) {
        self.status = self.current_status(block);
    }

    // returns true if this proposal is sure to pass (even before expiration if no future
    // sequence of possible votes can cause it to fail)
    pub fn is_passed(&self, block: &BlockInfo) -> bool {
        // we always require the quorum
        if self.votes.total() < votes_needed(self.total_weight, self.threshold.quorum) {
            return false;
        }
        if self.vote_ends_at.is_expired(block) {
            // If expired, we compare Yes votes against the total number of votes (minus abstain).
            let opinions = self.votes.total() - self.votes.abstain;
            self.votes.veto < votes_needed(self.votes.total(), self.threshold.veto_threshold)
                && self.votes.yes >= votes_needed(opinions, self.threshold.threshold)
        } else {
            // If not expired, we must assume all non-votes will be cast as No.
            // We compare threshold against the total weight (minus abstain).
            let possible_opinions = self.total_weight - self.votes.abstain;
            self.votes.veto < votes_needed(self.votes.total(), self.threshold.veto_threshold)
                && self.votes.yes >= votes_needed(possible_opinions, self.threshold.threshold)
        }
    }

    pub fn is_vetoed(&self) -> bool {
        self.votes.veto >= votes_needed(self.votes.total(), self.threshold.veto_threshold)
    }
}

// this is a helper function so Decimal works with u64 rather than Uint128
// also, we must *round up* here, as we need 8, not 7 votes to reach 50% of 15 total
fn votes_needed(weight: Uint128, percentage: Decimal) -> Uint128 {
    let applied = percentage * Uint128::from(PRECISION_FACTOR * weight.u128());
    // Divide by PRECISION_FACTOR, rounding up to the nearest integer
    Uint128::from((applied.u128() + PRECISION_FACTOR - 1) / PRECISION_FACTOR)
}

#[cfg(test)]
mod test {
    use cosmwasm_std::testing::mock_env;

    use super::*;

    #[test]
    fn count_votes() {
        let mut votes = Votes::new(Uint128::new(5));
        votes.submit(Vote::No, Uint128::new(10));
        votes.submit(Vote::Veto, Uint128::new(20));
        votes.submit(Vote::Yes, Uint128::new(30));
        votes.submit(Vote::Abstain, Uint128::new(40));

        assert_eq!(votes.total(), Uint128::new(105));
        assert_eq!(votes.yes, Uint128::new(35));
        assert_eq!(votes.no, Uint128::new(10));
        assert_eq!(votes.veto, Uint128::new(20));
        assert_eq!(votes.abstain, Uint128::new(40));
    }

    #[test]
    // we ensure this rounds up (as it calculates needed votes)
    fn votes_needed_rounds_properly() {
        // round up right below 1
        assert_eq!(
            Uint128::new(1),
            votes_needed(Uint128::new(3), Decimal::permille(333))
        );
        // round up right over 1
        assert_eq!(
            Uint128::new(2),
            votes_needed(Uint128::new(3), Decimal::permille(334))
        );
        assert_eq!(
            Uint128::new(11),
            votes_needed(Uint128::new(30), Decimal::permille(334))
        );

        // exact matches don't round
        assert_eq!(
            Uint128::new(17),
            votes_needed(Uint128::new(34), Decimal::percent(50))
        );
        assert_eq!(
            Uint128::new(12),
            votes_needed(Uint128::new(48), Decimal::percent(25))
        );
    }

    fn check_is_passed(
        threshold: Threshold,
        votes: Votes,
        total_weight: Uint128,
        is_expired: bool,
    ) -> bool {
        let block = mock_env().block;
        let expires = match is_expired {
            true => Expiration::AtHeight(block.height - 5),
            false => Expiration::AtHeight(block.height + 100),
        };
        let prop = Proposal {
            title: "Demo".to_string(),
            link: "Test".to_string(),
            description: "Info".to_string(),
            proposer: Addr::unchecked("test"),
            submitted_at: BlockTime {
                height: 100,
                time: Default::default(),
            },
            deposit_ends_at: Expiration::AtHeight(block.height - 20),
            vote_starts_at: BlockTime {
                height: block.height - 10,
                time: Default::default(),
            },
            vote_ends_at: expires,
            msgs: vec![],
            status: Status::Open,
            threshold,
            total_weight,
            votes,
            total_deposit: Default::default(),
            deposit_base_amount: Default::default(),
        };
        prop.is_passed(&block)
    }

    #[test]
    fn proposal_passed_quorum() {
        let quorum = Threshold {
            threshold: Decimal::percent(50),
            quorum: Decimal::percent(40),
            veto_threshold: Default::default(),
        };
        // all non-yes votes are counted for quorum
        let passing = Votes {
            yes: Uint128::new(7),
            no: Uint128::new(3),
            abstain: Uint128::new(2),
            veto: Uint128::new(1),
        };
        // abstain votes are not counted for threshold => yes / (yes + no + veto)
        let passes_ignoring_abstain = Votes {
            yes: Uint128::new(6),
            no: Uint128::new(4),
            abstain: Uint128::new(5),
            veto: Uint128::new(2),
        };
        // fails any way you look at it
        let failing = Votes {
            yes: Uint128::new(6),
            no: Uint128::new(5),
            abstain: Uint128::new(2),
            veto: Uint128::new(2),
        };

        // first, expired (voting period over)
        // over quorum (40% of 30 = 12), over threshold (7/11 > 50%)
        assert!(check_is_passed(
            quorum.clone(),
            passing.clone(),
            Uint128::new(30),
            true
        ));
        // under quorum it is not passing (40% of 33 = 13.2 > 13)
        assert!(!check_is_passed(
            quorum.clone(),
            passing.clone(),
            Uint128::new(33),
            true
        ));
        // over quorum, threshold passes if we ignore abstain
        // 17 total votes w/ abstain => 40% quorum of 40 total
        // 6 yes / (6 yes + 4 no + 2 votes) => 50% threshold
        assert!(check_is_passed(
            quorum.clone(),
            passes_ignoring_abstain.clone(),
            Uint128::new(40),
            true
        ));
        // over quorum, but under threshold fails also
        assert!(!check_is_passed(
            quorum.clone(),
            failing,
            Uint128::new(20),
            true
        ));

        // now, check with open voting period
        // would pass if closed, but fail here, as remaining votes no -> fail
        assert!(!check_is_passed(
            quorum.clone(),
            passing.clone(),
            Uint128::new(30),
            false
        ));
        assert!(!check_is_passed(
            quorum.clone(),
            passes_ignoring_abstain.clone(),
            Uint128::new(40),
            false
        ));
        // if we have threshold * total_weight as yes votes this must pass
        assert!(check_is_passed(
            quorum.clone(),
            passing.clone(),
            Uint128::new(14),
            false
        ));
        // all votes have been cast, some abstain
        assert!(check_is_passed(
            quorum.clone(),
            passes_ignoring_abstain,
            Uint128::new(17),
            false
        ));
        // 3 votes uncast, if they all vote no, we have 7 yes, 7 no+veto, 2 abstain (out of 16)
        assert!(check_is_passed(quorum, passing, Uint128::new(16), false));
    }

    #[test]
    fn quorum_edge_cases() {
        // when we pass absolute threshold (everyone else voting no, we pass), but still don't hit quorum
        let quorum = Threshold {
            threshold: Decimal::percent(60),
            quorum: Decimal::percent(80),
            veto_threshold: Decimal::percent(33),
        };

        // try 9 yes, 1 no (out of 15) -> 90% voter threshold, 60% absolute threshold, still no quorum
        // doesn't matter if expired or not
        let missing_voters = Votes {
            yes: Uint128::new(9),
            no: Uint128::new(1),
            abstain: Uint128::new(0),
            veto: Uint128::new(0),
        };
        assert!(!check_is_passed(
            quorum.clone(),
            missing_voters.clone(),
            Uint128::new(15),
            false
        ));
        assert!(!check_is_passed(
            quorum.clone(),
            missing_voters,
            Uint128::new(15),
            true
        ));

        // 1 less yes, 3 vetos and this passes only when expired
        let wait_til_expired = Votes {
            yes: Uint128::new(8),
            no: Uint128::new(1),
            abstain: Uint128::new(0),
            veto: Uint128::new(3),
        };
        assert!(!check_is_passed(
            quorum.clone(),
            wait_til_expired.clone(),
            Uint128::new(15),
            false
        ));
        assert!(check_is_passed(
            quorum.clone(),
            wait_til_expired,
            Uint128::new(15),
            true
        ));

        // 9 yes and 3 nos passes early
        let passes_early = Votes {
            yes: Uint128::new(9),
            no: Uint128::new(3),
            abstain: Uint128::new(0),
            veto: Uint128::new(0),
        };
        assert!(check_is_passed(
            quorum.clone(),
            passes_early.clone(),
            Uint128::new(15),
            false
        ));
        assert!(check_is_passed(
            quorum,
            passes_early,
            Uint128::new(15),
            true
        ));
    }
}
