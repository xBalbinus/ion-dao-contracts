use cosmwasm_std::{
    to_binary, Addr, BlockInfo, CosmosMsg, Decimal, Deps, Env, MessageInfo, StdError, StdResult,
    Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use cw_utils::{Duration, Expiration};

use crate::query::ProposalResponse;
use crate::state::{Proposal, STAKING_CONTRACT};
use crate::ContractError;

pub fn duration_to_expiry(block: &BlockInfo, period: &Duration) -> Expiration {
    match period {
        Duration::Height(height) => Expiration::AtHeight(block.height + height),
        Duration::Time(time) => Expiration::AtTime(block.time.plus_seconds(*time)),
    }
}

pub fn get_deposit_message(
    env: &Env,
    info: &MessageInfo,
    amount: &Uint128,
    gov_token: &Addr,
) -> StdResult<Vec<CosmosMsg>> {
    if *amount == Uint128::zero() {
        return Ok(vec![]);
    }
    let transfer_cw20_msg = Cw20ExecuteMsg::TransferFrom {
        owner: info.sender.clone().into(),
        recipient: env.contract.address.clone().into(),
        amount: *amount,
    };
    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: gov_token.into(),
        msg: to_binary(&transfer_cw20_msg)?,
        funds: vec![],
    };
    let cw20_transfer_cosmos_msg: CosmosMsg = exec_cw20_transfer.into();
    Ok(vec![cw20_transfer_cosmos_msg])
}

pub fn get_total_staked_supply(deps: Deps) -> StdResult<Uint128> {
    let staking_contract = STAKING_CONTRACT.load(deps.storage)?;

    // Get total supply
    let total: ion_stake::msg::TotalStakedAtHeightResponse = deps.querier.query_wasm_smart(
        staking_contract,
        &ion_stake::msg::QueryMsg::TotalStakedAtHeight { height: None },
    )?;
    Ok(total.total)
}

pub fn get_staked_balance(deps: Deps, address: Addr) -> StdResult<Uint128> {
    let staking_contract = STAKING_CONTRACT.load(deps.storage)?;

    // Get current staked balance
    let res: ion_stake::msg::StakedBalanceAtHeightResponse = deps.querier.query_wasm_smart(
        staking_contract,
        &ion_stake::msg::QueryMsg::StakedBalanceAtHeight {
            address: address.to_string(),
            height: None,
        },
    )?;
    Ok(res.balance)
}

pub fn get_voting_power_at_height(deps: Deps, address: Addr, height: u64) -> StdResult<Uint128> {
    let staking_contract = STAKING_CONTRACT.load(deps.storage)?;

    // Get voting power at height
    let balance: ion_stake::msg::StakedBalanceAtHeightResponse = deps.querier.query_wasm_smart(
        staking_contract,
        &ion_stake::msg::QueryMsg::StakedBalanceAtHeight {
            address: address.to_string(),
            height: Some(height),
        },
    )?;
    Ok(balance.balance)
}

pub fn proposal_to_response(block: &BlockInfo, id: u64, prop: Proposal) -> ProposalResponse {
    let status = prop.current_status(block);
    let total_weight = prop.total_weight;
    let total_votes = prop.votes.total();
    let quorum = Decimal::from_ratio(total_votes, total_weight);

    ProposalResponse {
        id,

        title: prop.title,
        link: prop.link,
        description: prop.description,
        proposer: prop.proposer,
        msgs: prop.msgs,
        status,

        deposit_starts_at: prop.deposit_starts_at,
        vote_starts_at: prop.vote_starts_at,
        expires_at: prop.expires_at,

        votes: prop.votes,
        quorum,
        threshold: prop.threshold,
        total_votes,
        total_weight,

        deposit_amount: prop.deposit,
    }
}

pub fn get_and_check_limit(limit: Option<u32>, max: u32, default: u32) -> StdResult<u32> {
    match limit {
        Some(l) => {
            if l <= max {
                Ok(l)
            } else {
                Err(StdError::generic_err(
                    ContractError::OversizedRequest {
                        size: l as u64,
                        max: max as u64,
                    }
                    .to_string(),
                ))
            }
        }
        None => Ok(default),
    }
}
