use cosmwasm_std::{
    coins, entry_point, to_binary, Addr, BankMsg, Binary, Deps, DepsMut, Empty, Env, MessageInfo,
    Order, QuerierWrapper, Reply, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Balance, BalanceResponse, Cw20CoinVerified, Cw20QueryMsg, Denom};
use cw3::{Status, Vote};
use cw_storage_plus::Bound;
use cw_utils::{maybe_addr, parse_reply_instantiate_data, Expiration, NativeBalance};

use crate::error::ContractError;
use crate::helpers::{
    duration_to_expiry, get_and_check_limit, get_total_staked_supply, get_voting_power_at_height,
    proposal_to_response,
};
use crate::msg::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, ProposalsQueryOption, ProposeMsg, QueryMsg, RangeOrder,
    VoteMsg,
};
use crate::proposal::BlockTime;
use crate::query::{
    ConfigResponse, ProposalResponse, ProposalsResponse, TokenBalancesResponse, TokenListResponse,
    VoteInfo, VoteResponse, VotesResponse,
};
use crate::state::{
    next_id, parse_id, Ballot, Config, Proposal, Votes, BALLOTS, CONFIG, DAO_PAUSED, GOV_TOKEN,
    IDX_PROPS_BY_PROPOSER, IDX_PROPS_BY_STATUS, PROPOSALS, STAKING_CONTRACT, TREASURY_TOKENS,
};

// Version info for migration info
pub const CONTRACT_NAME: &str = "crates.io:ion-dao";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

// Reply IDs
const INSTANTIATE_STAKING_CONTRACT_REPLY_ID: u64 = 0;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    msg.threshold.validate()?;

    let cfg = Config {
        name: msg.name,
        description: msg.description,
        threshold: msg.threshold,
        voting_period: msg.voting_period,
        deposit_period: msg.deposit_period,
        proposal_deposit: msg.proposal_deposit_amount,
        proposal_min_deposit: msg.proposal_deposit_min_amount,
    };
    CONFIG.save(deps.storage, &cfg)?;

    // Add native token to map of TREASURY TOKENS
    TREASURY_TOKENS.save(
        deps.storage,
        ("native", msg.gov_token.denom.as_str()),
        &Empty {},
    )?;

    // Save gov token
    GOV_TOKEN.save(deps.storage, &msg.gov_token.denom)?;

    // Instantiate staking contract with DAO as admin
    Ok(Response::new().add_submessage(SubMsg::reply_on_success(
        WasmMsg::Instantiate {
            code_id: msg.gov_token.stake_contract_code_id,
            funds: vec![],
            admin: Some(env.contract.address.to_string()),
            label: msg.gov_token.label,
            msg: to_binary(&ion_stake::msg::InstantiateMsg {
                admin: Some(env.contract.address),
                denom: msg.gov_token.denom,
                unstaking_duration: msg.gov_token.unstaking_duration,
            })?,
        },
        INSTANTIATE_STAKING_CONTRACT_REPLY_ID,
    )))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    match msg {
        ExecuteMsg::Propose(propose_msg) => execute_propose(deps, env, info, propose_msg),
        ExecuteMsg::Deposit { proposal_id } => execute_deposit(deps, env, info, proposal_id),
        ExecuteMsg::Vote(VoteMsg { proposal_id, vote }) => {
            execute_vote(deps, env, info, proposal_id, vote)
        }
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, env, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
        ExecuteMsg::PauseDAO { expiration } => execute_pause_dao(deps, env, info, expiration),
        ExecuteMsg::UpdateConfig(config) => execute_update_config(deps, env, info, config),
        ExecuteMsg::UpdateTokenList { to_add, to_remove } => {
            execute_update_token_list(deps, env, info, to_add, to_remove)
        }
        ExecuteMsg::UpdateStakingContract {
            new_staking_contract,
        } => execute_update_staking_contract(deps, env, info, new_staking_contract),
    }
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    propose_msg: ProposeMsg,
) -> Result<Response<Empty>, ContractError> {
    // Check if DAO is Paused
    let paused = DAO_PAUSED.may_load(deps.storage)?;
    if let Some(expiration) = paused {
        if !expiration.is_expired(&env.block) {
            return Err(ContractError::Paused {});
        }
    }

    let cfg = CONFIG.load(deps.storage)?;
    let gov_token = GOV_TOKEN.load(deps.storage)?;

    let received = cw_utils::may_pay(&info, gov_token.as_str())
        .map_err(|e| ContractError::Std(StdError::generic_err(format!("{:?}", e))))?;
    if received < cfg.proposal_min_deposit {
        return Err(ContractError::Unauthorized {});
    }

    let (status, expires_at) = if received < cfg.proposal_deposit {
        // to deposit period (pending)
        (
            Status::Pending,
            duration_to_expiry(&env.block, &cfg.deposit_period),
        )
    } else {
        // to voting period (open)
        (
            Status::Open,
            duration_to_expiry(&env.block, &cfg.voting_period),
        )
    };

    // Get total supply
    let total_supply = get_total_staked_supply(deps.as_ref())?;
    let now = BlockTime {
        height: env.block.height,
        time: env.block.time,
    };

    // Create a proposal
    let mut prop = Proposal {
        // payload
        title: propose_msg.title,
        link: propose_msg.link,
        description: propose_msg.description,
        proposer: info.sender.clone(),
        msgs: propose_msg.msgs,
        status,

        // time
        deposit_starts_at: now.clone(),
        vote_starts_at: if received < cfg.proposal_deposit {
            None
        } else {
            Some(now)
        },
        expires_at,

        // voting
        votes: Votes::default(),
        threshold: cfg.threshold,
        total_weight: total_supply,

        // deposit
        deposit: received,
    };
    prop.update_status(&env.block);
    let id = next_id(deps.storage)?;
    PROPOSALS.save(deps.storage, id, &prop)?;
    IDX_PROPS_BY_STATUS.save(deps.storage, (prop.status as u8, id), &Empty {})?;
    IDX_PROPS_BY_PROPOSER.save(deps.storage, (info.sender.clone(), id), &Empty {})?;

    Ok(Response::new()
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response<Empty>, ContractError> {
    // Check if DAO is Paused
    let paused = DAO_PAUSED.may_load(deps.storage)?;
    if let Some(expiration) = paused {
        if !expiration.is_expired(&env.block) {
            return Err(ContractError::Paused {});
        }
    }

    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    IDX_PROPS_BY_STATUS.remove(deps.storage, (prop.status as u8, proposal_id));
    if prop.status != Status::Pending {
        return Err(ContractError::WrongDepositStatus {});
    }

    let cfg = CONFIG.load(deps.storage)?;
    let gov_token = GOV_TOKEN.load(deps.storage)?;
    let received = cw_utils::may_pay(&info, gov_token.as_str())
        .map_err(|e| ContractError::Std(StdError::generic_err(format!("{:?}", e))))?;

    prop.update_status(&env.block);

    if prop.status == Status::Rejected {
        return Ok(Response::new()
            .add_message(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: info.funds,
            })
            .add_attributes(vec![
                ("action", "deposit"),
                ("result", "refund"),
                ("denom", gov_token.as_str()),
                ("amount", received.to_string().as_str()),
            ]));
    }

    prop.deposit = prop
        .deposit
        .checked_add(received)
        .map_err(|e| ContractError::Std(StdError::overflow(e)))?;
    if cfg.proposal_deposit <= prop.deposit {
        // open
        prop.status = Status::Open;
        prop.vote_starts_at = Some(BlockTime {
            height: env.block.height,
            time: env.block.time,
        });
        prop.expires_at = duration_to_expiry(&env.block, &cfg.voting_period);
    }

    PROPOSALS.save(deps.storage, proposal_id, &prop)?;
    IDX_PROPS_BY_STATUS.save(deps.storage, (prop.status as u8, proposal_id), &Empty {})?;

    Ok(Response::new().add_attributes(vec![
        ("action", "deposit"),
        ("result", "processed"),
        ("denom", gov_token.as_str()),
        ("amount", received.to_string().as_str()),
        ("proposal_id", proposal_id.to_string().as_str()),
    ]))
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<Response<Empty>, ContractError> {
    // Check if DAO is Paused
    let paused = DAO_PAUSED.may_load(deps.storage)?;
    if let Some(expiration) = paused {
        if !expiration.is_expired(&env.block) {
            return Err(ContractError::Paused {});
        }
    }

    // Ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    IDX_PROPS_BY_STATUS.remove(deps.storage, (prop.status as u8, proposal_id));

    if prop.status != Status::Open {
        return Err(ContractError::NotOpen {});
    }
    if prop.expires_at.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // Get voter balance at proposal start
    let vote_power = get_voting_power_at_height(
        deps.as_ref(),
        info.sender.clone(),
        prop.vote_starts_at.as_ref().unwrap().height,
    )?;

    if vote_power == Uint128::zero() {
        return Err(ContractError::Unauthorized {});
    }

    // Cast vote
    let ballot = BALLOTS.may_load(deps.storage, (proposal_id, &info.sender))?;
    if let Some(bal) = ballot {
        // cancel vote
        prop.votes.revoke(bal.vote, bal.weight);
    }

    BALLOTS.save(
        deps.storage,
        (proposal_id, &info.sender),
        &Ballot {
            weight: vote_power,
            vote,
        },
    )?;

    // Update vote tally
    prop.votes.submit(vote, vote_power);
    prop.update_status(&env.block);
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;
    IDX_PROPS_BY_STATUS.save(deps.storage, (prop.status as u8, proposal_id), &Empty {})?;

    Ok(Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // Check if DAO is Paused
    let paused = DAO_PAUSED.may_load(deps.storage)?;
    if let Some(expiration) = paused {
        if !expiration.is_expired(&env.block) {
            return Err(ContractError::Paused {});
        }
    }

    let gov_token = GOV_TOKEN.load(deps.storage)?;

    // Anyone can trigger this if the vote passed
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    IDX_PROPS_BY_STATUS.remove(deps.storage, (prop.status as u8, proposal_id));
    // We allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    if prop.current_status(&env.block) != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // Set it to executed
    prop.status = Status::Executed;
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;
    IDX_PROPS_BY_STATUS.save(deps.storage, (prop.status as u8, proposal_id), &Empty {})?;

    // Dispatch all proposed messages
    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: prop.proposer.into(),
            amount: coins(prop.deposit.u128(), gov_token),
        })
        .add_messages(prop.msgs)
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response<Empty>, ContractError> {
    // Check if DAO is Paused
    let paused = DAO_PAUSED.may_load(deps.storage)?;
    if let Some(expiration) = paused {
        if !expiration.is_expired(&env.block) {
            return Err(ContractError::Paused {});
        }
    }

    let gov_token = GOV_TOKEN.load(deps.storage)?;

    // Anyone can trigger this if the vote passed
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    IDX_PROPS_BY_STATUS.remove(deps.storage, (prop.status as u8, proposal_id));
    if [Status::Executed, Status::Rejected, Status::Passed]
        .iter()
        .any(|x| *x == prop.status)
    {
        return Err(ContractError::WrongCloseStatus {});
    }
    if !prop.expires_at.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // Set it to failed
    prop.status = Status::Rejected;
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;
    IDX_PROPS_BY_STATUS.save(deps.storage, (prop.status as u8, proposal_id), &Empty {})?;

    let mut resp = Response::new();
    if !prop.is_vetoed() {
        resp = resp.add_message(BankMsg::Send {
            to_address: prop.proposer.to_string(),
            amount: coins(prop.deposit.u128(), gov_token),
        });
    }

    let log_result = if prop.is_vetoed() {
        "vetoed"
    } else {
        "refunded"
    };

    Ok(resp.add_attributes(vec![
        ("action", "close"),
        ("result", log_result),
        ("sender", info.sender.to_string().as_str()),
        ("proposal_id", proposal_id.to_string().as_str()),
    ]))
}

pub fn execute_pause_dao(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    expiration: Expiration,
) -> Result<Response<Empty>, ContractError> {
    // Only contract can call this method
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    DAO_PAUSED.save(deps.storage, &expiration)?;

    Ok(Response::new()
        .add_attribute("action", "pause_dao")
        .add_attribute("expiration", expiration.to_string()))
}

pub fn execute_update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    update_config_msg: Config,
) -> Result<Response<Empty>, ContractError> {
    // Only contract can call this method
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    update_config_msg.threshold.validate()?;

    CONFIG.save(deps.storage, &update_config_msg)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("sender", info.sender))
}

pub fn execute_update_staking_contract(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_staking_contract: Addr,
) -> Result<Response<Empty>, ContractError> {
    // Only contract can call this method
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }
    let new_staking_contract = deps.api.addr_validate(new_staking_contract.as_str())?;

    // Replace the existing staking contract
    STAKING_CONTRACT.save(deps.storage, &new_staking_contract)?;

    Ok(Response::new()
        .add_attribute("action", "update_staking_contract")
        .add_attribute("new_staking_contract", new_staking_contract))
}

pub fn execute_update_token_list(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    to_add: Vec<Denom>,
    to_remove: Vec<Denom>,
) -> Result<Response<Empty>, ContractError> {
    // Only contract can call this method
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    // Limit the number of token modifications that can occur in one
    // execution to prevent out of gas issues.
    if to_add.len() + to_remove.len() > MAX_LIMIT as usize {
        return Err(ContractError::OversizedRequest {
            size: (to_add.len() + to_remove.len()) as u64,
            max: MAX_LIMIT as u64,
        });
    }

    for token in &to_add {
        match token {
            Denom::Native(native_denom) => {
                TREASURY_TOKENS.save(deps.storage, ("native", native_denom.as_str()), &Empty {})?
            }
            Denom::Cw20(cw20_addr) => {
                TREASURY_TOKENS.save(deps.storage, ("cw20", cw20_addr.as_str()), &Empty {})?
            }
        }
    }

    for token in &to_remove {
        match token {
            Denom::Native(native_denom) => {
                TREASURY_TOKENS.remove(deps.storage, ("native", native_denom.as_str()))
            }
            Denom::Cw20(cw20_addr) => {
                TREASURY_TOKENS.remove(deps.storage, ("cw20", cw20_addr.as_str()))
            }
        }
    }

    Ok(Response::new().add_attribute("action", "update_cw20_token_list"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::TokenList {} => to_binary(&query_token_list(deps)),
        QueryMsg::TokenBalances {
            start,
            limit,
            order,
        } => to_binary(&query_token_balances(deps, env, start, limit, order)?),

        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        QueryMsg::Proposals {
            query,
            start,
            limit,
            order,
        } => to_binary(&query_proposals(deps, env, query, start, limit, order)?),
        QueryMsg::ProposalCount {} => to_binary(&query_proposal_count(deps)),

        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::Votes {
            proposal_id,
            start,
            limit,
            order,
        } => to_binary(&query_votes(deps, proposal_id, start, limit, order)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    let gov_token = GOV_TOKEN.load(deps.storage)?;
    let staking_contract = STAKING_CONTRACT.load(deps.storage)?;

    Ok(ConfigResponse {
        config,
        gov_token,
        staking_contract,
    })
}

fn query_token_list(deps: Deps) -> TokenListResponse {
    let token_list: Vec<Denom> = TREASURY_TOKENS
        .keys(deps.storage, None, None, Order::Ascending)
        .map(|item| -> Denom {
            let (k1, k2) = item.unwrap();
            match k1.as_str() {
                "native" => Denom::Native(k2),
                "cw20" => Denom::Cw20(deps.api.addr_validate(k2.as_str()).unwrap()),
                _ => panic!("invalid asset type {}", k1),
            }
        })
        .collect();

    TokenListResponse { token_list }
}

fn query_token_balances(
    deps: Deps,
    env: Env,
    start: Option<Denom>,
    limit: Option<u32>,
    order: Option<RangeOrder>,
) -> StdResult<TokenBalancesResponse> {
    let limit = get_and_check_limit(limit, MAX_LIMIT, DEFAULT_LIMIT)? as usize;
    let order = order.unwrap_or(RangeOrder::Asc).into();
    let start = start.map(|v| match v {
        Denom::Native(denom) => ("native", denom),
        Denom::Cw20(addr) => ("cw20", addr.to_string()),
    });

    let store = deps.storage;
    let querier = deps.querier;
    let balances: StdResult<Vec<_>> = if let Some((prefix, start)) = start {
        let (min, max) = match order {
            Order::Ascending => (Some(Bound::exclusive(start.as_bytes())), None),
            Order::Descending => (None, Some(Bound::exclusive(start.as_bytes()))),
        };
        TREASURY_TOKENS
            .prefix(prefix)
            .keys(store, min, max, order)
            .take(limit)
            .map(|v| query_balance_with_asset_type(querier, env.clone(), prefix, v?.as_str()))
            .collect()
    } else {
        TREASURY_TOKENS
            .keys(store, None, None, order)
            .take(limit)
            .map(|item| {
                let (k1, k2) = item?;
                query_balance_with_asset_type(querier, env.clone(), &k1, &k2)
            })
            .collect()
    };

    Ok(TokenBalancesResponse {
        balances: balances?,
    })
}

fn query_balance_with_asset_type(
    querier: QuerierWrapper,
    env: Env,
    asset_type: &str,
    value: &str,
) -> StdResult<Balance> {
    match asset_type {
        "native" => {
            let balance_resp = querier.query_balance(env.contract.address, value).unwrap();

            Ok(Balance::Native(NativeBalance(vec![balance_resp])))
        }
        "cw20" => {
            let balance_resp: BalanceResponse = querier
                .query_wasm_smart(
                    value,
                    &Cw20QueryMsg::Balance {
                        address: env.contract.address.to_string(),
                    },
                )
                .unwrap_or(BalanceResponse {
                    balance: Uint128::zero(),
                });

            Ok(Balance::Cw20(Cw20CoinVerified {
                address: Addr::unchecked(value),
                amount: balance_resp.balance,
            }))
        }
        _ => Err(StdError::generic_err(format!(
            "invalid asset type {}",
            asset_type
        ))),
    }
}

fn query_proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse> {
    let prop = PROPOSALS.load(deps.storage, id)?;
    Ok(proposal_to_response(&env.block, id, prop))
}

fn query_proposals(
    deps: Deps,
    env: Env,
    query: ProposalsQueryOption,
    start: Option<u64>,
    limit: Option<u32>,
    order: Option<RangeOrder>,
) -> StdResult<ProposalsResponse> {
    let limit = get_and_check_limit(limit, MAX_LIMIT, DEFAULT_LIMIT)? as usize;
    let order = order.unwrap_or(RangeOrder::Asc).into();
    let (min, max) = match order {
        Order::Ascending => (start.map(Bound::exclusive_int), None),
        Order::Descending => (None, start.map(Bound::exclusive_int)),
    };

    let props: StdResult<Vec<_>> = match query {
        ProposalsQueryOption::FindByStatus { status } => IDX_PROPS_BY_STATUS
            .prefix(status as u8)
            .range(deps.storage, min, max, order)
            .take(limit)
            .map(|item| {
                let (k, _) = item.unwrap();
                Ok(proposal_to_response(
                    &env.block,
                    k,
                    PROPOSALS.load(deps.storage, k).unwrap(),
                ))
            })
            .collect(),
        ProposalsQueryOption::FindByProposer { proposer } => IDX_PROPS_BY_PROPOSER
            .prefix(proposer)
            .range(deps.storage, min, max, order)
            .take(limit)
            .map(|item| {
                let (k, _) = item.unwrap();
                Ok(proposal_to_response(
                    &env.block,
                    k,
                    PROPOSALS.load(deps.storage, k).unwrap(),
                ))
            })
            .collect(),
        ProposalsQueryOption::Everything {} => PROPOSALS
            .range_raw(deps.storage, min, max, order)
            .take(limit)
            .map(|item| {
                let (k, prop) = item.unwrap();
                Ok(proposal_to_response(
                    &env.block,
                    parse_id(k.as_slice())?,
                    prop,
                ))
            })
            .collect(),
    };

    Ok(ProposalsResponse { proposals: props? })
}

fn query_proposal_count(deps: Deps) -> u64 {
    PROPOSALS
        .keys(deps.storage, None, None, Order::Descending)
        .count() as u64
}

fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let prop = BALLOTS.may_load(deps.storage, (proposal_id, &voter_addr))?;
    let vote = prop.map(|b| VoteInfo {
        voter,
        vote: b.vote,
        weight: b.weight,
    });
    Ok(VoteResponse { vote })
}

fn query_votes(
    deps: Deps,
    proposal_id: u64,
    start: Option<String>,
    limit: Option<u32>,
    order: Option<RangeOrder>,
) -> StdResult<VotesResponse> {
    let limit = get_and_check_limit(limit, MAX_LIMIT, DEFAULT_LIMIT)? as usize;
    let order = order.unwrap_or(RangeOrder::Asc).into();
    let start = maybe_addr(deps.api, start)?;
    let (min, max) = match order {
        Order::Ascending => (start.map(|addr| Bound::exclusive(addr.as_ref())), None),
        Order::Descending => (None, start.map(|addr| Bound::exclusive(addr.as_ref()))),
    };

    let votes: StdResult<Vec<_>> = BALLOTS
        .prefix(proposal_id)
        .range_raw(deps.storage, min, max, order)
        .take(limit)
        .map(|item| {
            let (voter, ballot) = item?;
            Ok(VoteInfo {
                voter: String::from_utf8(voter)?,
                vote: ballot.vote,
                weight: ballot.weight,
            })
        })
        .collect();

    Ok(VotesResponse { votes: votes? })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_STAKING_CONTRACT_REPLY_ID => {
            let res = parse_reply_instantiate_data(msg);
            match res {
                Ok(res) => {
                    // Validate contract address
                    let staking_contract_addr = deps.api.addr_validate(&res.contract_address)?;

                    // Save gov token
                    STAKING_CONTRACT.save(deps.storage, &staking_contract_addr)?;

                    Ok(Response::new())
                }
                Err(_) => Err(ContractError::InstantiateGovTokenError {}),
            }
        }
        _ => Err(ContractError::UnknownReplyId { id: msg.id }),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    // No state migrations performed, just returned a Response
    Ok(Response::default())
}
