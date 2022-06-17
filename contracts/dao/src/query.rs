use cosmwasm_std::{Addr, Env, Order, StdError, StdResult, Uint128};
use cw20::{Balance, BalanceResponse, Cw20CoinVerified, Cw20QueryMsg, Denom};
use cw_storage_plus::Bound;
use cw_utils::{maybe_addr, NativeBalance};
use osmo_bindings::OsmosisMsg;

use crate::helpers::{get_and_check_limit, proposal_to_response};
use crate::msg::{
    ConfigResponse, DepositResponse, DepositsQueryOption, DepositsResponse, ProposalResponse,
    ProposalsQueryOption, ProposalsResponse, RangeOrder, TokenBalancesResponse, TokenListResponse,
    VoteInfo, VoteResponse, VotesResponse,
};
use crate::state::{
    parse_id, BALLOTS, CONFIG, DEPOSITS, GOV_TOKEN, IDX_DEPOSITS_BY_DEPOSITOR,
    IDX_PROPS_BY_PROPOSER, IDX_PROPS_BY_STATUS, PROPOSALS, STAKING_CONTRACT, TREASURY_TOKENS,
};
use crate::{Deps, QuerierWrapper, DEFAULT_LIMIT, MAX_LIMIT};

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

pub fn config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    let gov_token = GOV_TOKEN.load(deps.storage)?;
    let staking_contract = STAKING_CONTRACT.load(deps.storage)?;

    Ok(ConfigResponse {
        config,
        gov_token,
        staking_contract,
    })
}

pub fn token_list(deps: Deps) -> TokenListResponse {
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

pub fn token_balances(
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
            Order::Ascending => (Some(Bound::<&str>::exclusive(start.as_str())), None),
            Order::Descending => (None, Some(Bound::<&str>::exclusive(start.as_str()))),
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

pub fn proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse<OsmosisMsg>> {
    let prop = PROPOSALS.load(deps.storage, id)?;
    Ok(proposal_to_response(&env.block, id, prop))
}

pub fn proposals(
    deps: Deps,
    env: Env,
    query: ProposalsQueryOption,
    start: Option<u64>,
    limit: Option<u32>,
    order: Option<RangeOrder>,
) -> StdResult<ProposalsResponse<OsmosisMsg>> {
    let limit = get_and_check_limit(limit, MAX_LIMIT, DEFAULT_LIMIT)? as usize;
    let order = order.unwrap_or(RangeOrder::Asc).into();
    let (min, max) = match order {
        Order::Ascending => (start.map(Bound::exclusive), None),
        Order::Descending => (None, start.map(Bound::exclusive)),
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

pub fn proposal_count(deps: Deps) -> u64 {
    PROPOSALS
        .keys(deps.storage, None, None, Order::Descending)
        .count() as u64
}

pub fn vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
    let voter_addr = deps.api.addr_validate(&voter)?;
    let prop = BALLOTS.may_load(deps.storage, (proposal_id, &voter_addr))?;
    let vote = prop.map(|b| VoteInfo {
        voter,
        vote: b.vote,
        weight: b.weight,
    });
    Ok(VoteResponse { vote })
}

pub fn votes(
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
        Order::Ascending => (start.as_ref().map(Bound::<&Addr>::exclusive), None),
        Order::Descending => (None, start.as_ref().map(Bound::<&Addr>::exclusive)),
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

pub fn deposit(deps: Deps, proposal_id: u64, depositor: String) -> StdResult<DepositResponse> {
    let depositor = deps.api.addr_validate(depositor.as_str())?;
    let deposit = DEPOSITS.load(deps.storage, (proposal_id, depositor.clone()))?;

    Ok(DepositResponse {
        proposal_id,
        depositor: depositor.to_string(),
        amount: deposit,
    })
}

pub fn deposits(
    deps: Deps,
    query: DepositsQueryOption,
    limit: Option<u32>,
    order: Option<RangeOrder>,
) -> StdResult<DepositsResponse> {
    let limit = get_and_check_limit(limit, MAX_LIMIT, DEFAULT_LIMIT)? as usize;
    let order = order.unwrap_or(RangeOrder::Asc).into();

    let deposits: StdResult<Vec<_>> = match query {
        DepositsQueryOption::FindByProposal { proposal_id, start } => {
            let start = maybe_addr(deps.api, start)?;
            let (min, max) = match order {
                Order::Ascending => (start.map(Bound::<Addr>::exclusive), None),
                Order::Descending => (None, start.map(Bound::<Addr>::exclusive)),
            };

            DEPOSITS
                .prefix(proposal_id)
                .range(deps.storage, min, max, order)
                .take(limit)
                .map(|item| {
                    let (depositor, amount) = item?;
                    Ok(DepositResponse {
                        proposal_id,
                        depositor: depositor.to_string(),
                        amount,
                    })
                })
                .collect()
        }
        DepositsQueryOption::FindByDepositor { depositor, start } => {
            let depositor = deps.api.addr_validate(depositor.as_str())?;
            let (min, max) = match order {
                Order::Ascending => (start.map(Bound::exclusive), None),
                Order::Descending => (None, start.map(Bound::exclusive)),
            };

            IDX_DEPOSITS_BY_DEPOSITOR
                .prefix(depositor.clone())
                .range(deps.storage, min, max, order)
                .take(limit)
                .map(|item| {
                    let (proposal_id, _) = item?;
                    let deposit = DEPOSITS.load(deps.storage, (proposal_id, depositor.clone()))?;

                    Ok(DepositResponse {
                        proposal_id,
                        depositor: depositor.to_string(),
                        amount: deposit,
                    })
                })
                .collect()
        }
        DepositsQueryOption::Everything { start } => {
            let start = start
                .map(|(id, addr)| -> StdResult<(u64, Addr)> {
                    let addr = deps.api.addr_validate(&addr)?;

                    Ok((id, addr))
                })
                .transpose()?;
            let (min, max) = match order {
                Order::Ascending => (start.map(Bound::exclusive), None),
                Order::Descending => (None, start.map(Bound::exclusive)),
            };

            DEPOSITS
                .range(deps.storage, min, max, order)
                .take(limit)
                .map(|item| {
                    let ((proposal_id, depositor), deposit) = item?;

                    Ok(DepositResponse {
                        proposal_id,
                        depositor: depositor.to_string(),
                        amount: deposit,
                    })
                })
                .collect()
        }
    };

    Ok(DepositsResponse {
        deposits: deposits?,
    })
}
