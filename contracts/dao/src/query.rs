use std::fmt;

use cosmwasm_std::{Addr, CosmosMsg, Decimal, Empty, Uint128};
use cw20::{Balance, Denom};
use cw3::{Status, Vote};
use cw_utils::Expiration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::{BlockTime, Config, Threshold, Votes};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ConfigResponse {
    pub config: Config,
    pub gov_token: String,
    pub staking_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TokenBalancesResponse {
    pub balances: Vec<Balance>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TokenListResponse {
    pub token_list: Vec<Denom>,
}

/// Note, if you are storing custom messages in the proposal,
/// the querier needs to know what possible custom message types
/// those are in order to parse the response
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProposalResponse<T = Empty>
where
    T: Clone + fmt::Debug + PartialEq + JsonSchema,
{
    pub id: u64,

    // payload
    pub title: String,
    pub link: String,
    pub description: String,
    pub proposer: Addr,
    pub msgs: Vec<CosmosMsg<T>>,
    pub status: Status,

    // time
    pub deposit_starts_at: BlockTime,
    pub vote_starts_at: Option<BlockTime>,
    pub expires_at: Expiration,

    // vote tally
    pub votes: Votes,
    pub quorum: Decimal,
    pub threshold: Threshold,
    pub total_votes: Uint128,
    pub total_weight: Uint128,

    // deposit
    pub deposit_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ProposalsResponse {
    pub proposals: Vec<ProposalResponse>,
}

/// Returns the vote (opinion as well as weight counted) as well as
/// the address of the voter who submitted it
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteInfo {
    pub voter: String,
    pub vote: Vote,
    pub weight: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteResponse {
    pub vote: Option<VoteInfo>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VotesResponse {
    pub votes: Vec<VoteInfo>,
}
