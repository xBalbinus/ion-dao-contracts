#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Empty, Env, MessageInfo, Order, Reply, StdError, StdResult, Uint128,
    WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Balance, BalanceResponse, Cw20CoinVerified, Cw20QueryMsg, Denom};
use cw_storage_plus::Bound;
use cw_utils::{maybe_addr, parse_reply_instantiate_data, NativeBalance};
use osmo_bindings::OsmosisMsg;

use crate::error::ContractError;
use crate::helpers::{get_and_check_limit, get_config, proposal_to_response};
use crate::msg::{
    ExecuteMsg, GovToken, InstantiateMsg, MigrateMsg, ProposalsQueryOption, QueryMsg, RangeOrder,
    VoteMsg,
};
use crate::query::{
    ConfigResponse, ProposalResponse, ProposalsResponse, TokenBalancesResponse, TokenListResponse,
    VoteInfo, VoteResponse, VotesResponse,
};
use crate::state::{
    parse_id, Config, BALLOTS, CONFIG, GOV_TOKEN, IDX_PROPS_BY_PROPOSER, IDX_PROPS_BY_STATUS,
    PROPOSALS, STAKING_CONTRACT, TREASURY_TOKENS,
};
use crate::{Deps, DepsMut, QuerierWrapper, Response, SubMsg, DEFAULT_LIMIT, MAX_LIMIT};

// Version info for migration info
pub const CONTRACT_NAME: &str = "crates.io:ion-dao";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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

    match msg.gov_token {
        GovToken::Create {
            denom,
            label,
            stake_contract_code_id,
            unstaking_duration,
        } => {
            // Add native token to map of TREASURY TOKENS
            TREASURY_TOKENS.save(deps.storage, ("native", denom.as_str()), &Empty {})?;

            // Save gov token
            GOV_TOKEN.save(deps.storage, &denom)?;

            // Instantiate staking contract with DAO as admin
            Ok(Response::new().add_submessage(SubMsg::reply_on_success(
                WasmMsg::Instantiate {
                    code_id: stake_contract_code_id,
                    funds: vec![],
                    admin: Some(env.contract.address.to_string()),
                    label,
                    msg: to_binary(&ion_stake::msg::InstantiateMsg {
                        admin: Some(env.contract.address),
                        denom,
                        unstaking_duration,
                    })?,
                },
                INSTANTIATE_STAKING_CONTRACT_REPLY_ID,
            )))
        }

        GovToken::Reuse { stake_contract } => {
            let addr = deps.api.addr_validate(stake_contract.as_str())?;
            STAKING_CONTRACT.save(deps.storage, &addr)?;

            let staking_config = get_config(deps.as_ref())?;
            // Add native token to map of TREASURY TOKENS
            TREASURY_TOKENS.save(
                deps.storage,
                ("native", staking_config.denom.as_str()),
                &Empty {},
            )?;

            // Save gov token
            GOV_TOKEN.save(deps.storage, &staking_config.denom)?;

            Ok(Response::new())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    use crate::execute;
    use crate::msg::ExecuteMsg::*;

    match msg {
        Propose(propose_msg) => execute::propose(deps, env, info, propose_msg),
        Deposit { proposal_id } => execute::deposit(deps, env, info, proposal_id),
        Vote(VoteMsg { proposal_id, vote }) => execute::vote(deps, env, info, proposal_id, vote),
        Execute { proposal_id } => execute::execute(deps, env, info, proposal_id),
        Close { proposal_id } => execute::close(deps, env, info, proposal_id),
        PauseDAO { expiration } => execute::pause_dao(deps, env, info, expiration),
        UpdateConfig(config) => execute::update_config(deps, env, info, config),
        UpdateTokenList { to_add, to_remove } => {
            execute::update_token_list(deps, env, info, to_add, to_remove)
        }
        UpdateStakingContract {
            new_staking_contract,
        } => execute::update_staking_contract(deps, env, info, new_staking_contract),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    use crate::msg::QueryMsg::*;

    match msg {
        GetConfig {} => to_binary(&query::config(deps)?),
        TokenList {} => to_binary(&query::token_list(deps)),
        TokenBalances {
            start,
            limit,
            order,
        } => to_binary(&query::token_balances(deps, env, start, limit, order)?),

        Proposal { proposal_id } => to_binary(&query::proposal(deps, env, proposal_id)?),
        Proposals {
            query,
            start,
            limit,
            order,
        } => to_binary(&query::proposals(deps, env, query, start, limit, order)?),
        ProposalCount {} => to_binary(&query::proposal_count(deps)),

        Vote { proposal_id, voter } => to_binary(&query::vote(deps, proposal_id, voter)?),
        Votes {
            proposal_id,
            start,
            limit,
            order,
        } => to_binary(&query::votes(deps, proposal_id, start, limit, order)?),

        Deposit {
            proposal_id,
            depositor,
        } => to_binary(&query::deposit(deps, proposal_id, depositor)?),
        Deposits {
            query,
            limit,
            order,
        } => to_binary(&query::deposits(deps, query, limit, order)?),
    }
}

mod query {
    use crate::msg::DepositsQueryOption;
    use crate::query::{DepositResponse, DepositsResponse};
    use crate::state::{DEPOSITS, IDX_DEPOSITS_BY_DEPOSITOR};

    use super::*;

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

    pub fn balance_with_asset_type(
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
            Order::Ascending => (
                start.as_ref().map(|addr| Bound::<&Addr>::exclusive(addr)),
                None,
            ),
            Order::Descending => (
                None,
                start.as_ref().map(|addr| Bound::<&Addr>::exclusive(addr)),
            ),
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
                        let deposit =
                            DEPOSITS.load(deps.storage, (proposal_id, depositor.clone()))?;

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
