extern crate core;

use osmo_bindings::{OsmosisMsg, OsmosisQuery};

pub use crate::error::ContractError;

// Type aliases
pub type Response = cosmwasm_std::Response<OsmosisMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<OsmosisMsg>;
pub type CosmosMsg = cosmwasm_std::CosmosMsg<OsmosisMsg>;
pub type Deps<'a> = cosmwasm_std::Deps<'a, OsmosisQuery>;
pub type DepsMut<'a> = cosmwasm_std::DepsMut<'a, OsmosisQuery>;
pub type QuerierWrapper<'a> = cosmwasm_std::QuerierWrapper<'a, OsmosisQuery>;

// Settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub mod contract;
mod error;
mod execute;
pub mod helpers;
pub mod msg;
pub mod proposal;
pub mod query;
pub mod state;
pub mod threshold;

#[cfg(test)]
mod tests;
