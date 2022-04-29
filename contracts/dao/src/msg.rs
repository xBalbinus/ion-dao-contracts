use cosmwasm_std::{Addr, CosmosMsg, Empty, Order, Uint128};
use cw20::Denom;
use cw3::Vote;
use cw_utils::{Duration, Expiration};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::Config;
use crate::threshold::Threshold;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    // The name of the DAO.
    pub name: String,
    // A description of the DAO.
    pub description: String,
    /// Set an existing governance token or launch a new one
    pub gov_token: GovToken,
    /// Voting params configuration
    pub threshold: Threshold,

    pub voting_period: Duration,

    pub deposit_period: Duration,

    /// Deposit required to make a proposal
    pub proposal_deposit_amount: Uint128,
    pub proposal_deposit_min_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GovToken {
    pub denom: String,
    pub label: String,
    pub stake_contract_code_id: u64,
    pub unstaking_duration: Option<Duration>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ProposeMsg {
    pub title: String,
    pub link: String,
    pub description: String,
    pub msgs: Vec<CosmosMsg<Empty>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VoteMsg {
    pub proposal_id: u64,
    pub vote: Vote,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Makes a new proposal
    Propose(ProposeMsg),
    Deposit {
        proposal_id: u64,
    },
    /// Vote on an open proposal
    Vote(VoteMsg),
    /// Execute a passed proposal
    Execute {
        proposal_id: u64,
    },
    /// Close a failed proposal
    Close {
        proposal_id: u64,
    },
    /// Pauses DAO governance (can only be called by DAO contract)
    PauseDAO {
        expiration: Expiration,
    },
    /// Update DAO config (can only be called by DAO contract)
    UpdateConfig(Config),
    /// Updates token list
    UpdateTokenList {
        to_add: Vec<Denom>,
        to_remove: Vec<Denom>,
    },
    /// Update Staking Contract (can only be called by DAO contract)
    /// WARNING: this changes the contract controlling voting
    UpdateStakingContract {
        new_staking_contract: Addr,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RangeOrder {
    Asc,
    Desc,
}

impl From<RangeOrder> for Order {
    fn from(order: RangeOrder) -> Self {
        match order {
            RangeOrder::Asc => Order::Ascending,
            RangeOrder::Desc => Order::Descending,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns Config
    GetConfig {},
    /// Return list of cw20 Tokens associated with the DAO Treasury
    TokenList {},
    /// Returns All DAO Cw20 Balances
    TokenBalances {
        start: Option<Denom>,
        limit: Option<u32>,
        order: Option<RangeOrder>,
    },

    /// Returns ProposalResponse
    Proposal { proposal_id: u64 },
    /// Returns ProposalListResponse
    Proposals {
        start: Option<u64>,
        limit: Option<u32>,
        order: Option<RangeOrder>,
    },
    /// Returns the number of proposals in the DAO (u64)
    ProposalCount {},
    /// Returns VoteResponse
    Vote { proposal_id: u64, voter: String },
    /// Returns VoteListResponse
    Votes {
        proposal_id: u64,
        start: Option<String>,
        limit: Option<u32>,
        order: Option<RangeOrder>,
    },
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::to_vec;

    use super::*;

    #[test]
    fn vote_encoding() {
        let a = Vote::Yes;
        let encoded = to_vec(&a).unwrap();
        let json = String::from_utf8_lossy(&encoded).to_string();
        assert_eq!(r#""yes""#, json.as_str());
    }

    #[test]
    fn vote_encoding_embedded() {
        let msg = ExecuteMsg::Vote(VoteMsg {
            proposal_id: 17,
            vote: Vote::No,
        });
        let encoded = to_vec(&msg).unwrap();
        let json = String::from_utf8_lossy(&encoded).to_string();
        assert_eq!(r#"{"vote":{"proposal_id":17,"vote":"no"}}"#, json.as_str());
    }
}
