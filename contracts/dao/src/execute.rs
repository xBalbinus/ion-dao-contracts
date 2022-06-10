use std::ops::Add;

use cosmwasm_std::{
    coins, Addr, BankMsg, BlockInfo, Empty, Env, MessageInfo, Order, StdError, StdResult, Storage,
    Uint128,
};
use cw20::Denom;
use cw3::{Status, Vote};
use cw_storage_plus::Bound;
use cw_utils::{may_pay, Expiration};

use crate::helpers::{
    duration_to_expiry, get_and_check_limit, get_total_staked_supply, get_voting_power_at_height,
};
use crate::msg::ProposeMsg;
use crate::state::{
    next_id, Ballot, Config, Proposal, Votes, BALLOTS, CONFIG, DAO_PAUSED, DEPOSITS, GOV_TOKEN,
    IDX_DEPOSITS_BY_DEPOSITOR, IDX_PROPS_BY_PROPOSER, IDX_PROPS_BY_STATUS, PROPOSALS,
    STAKING_CONTRACT, TREASURY_TOKENS,
};
use crate::ContractError;

use super::{CosmosMsg, DepsMut, Response, SubMsg, MAX_LIMIT};

fn check_paused(storage: &dyn Storage, block: &BlockInfo) -> Result<(), ContractError> {
    let paused = DAO_PAUSED.may_load(storage)?;
    if let Some(expiration) = paused {
        if !expiration.is_expired(block) {
            return Err(ContractError::Paused {});
        }
    }

    Ok(())
}

fn check_proposal_status(proposal: &Proposal, desired_status: Status) -> Result<(), ContractError> {
    if !proposal.status.eq(&desired_status) {
        return Err(ContractError::InvalidProposalStatus {
            current: format!("{:?}", proposal.status),
            desired: format!("{:?}", desired_status),
        });
    }

    Ok(())
}

fn create_proposal(
    storage: &mut dyn Storage,
    prop_id: u64,
    proposer: &Addr,
    proposal: &Proposal,
) -> StdResult<()> {
    PROPOSALS.save(storage, prop_id, proposal)?;
    IDX_PROPS_BY_STATUS.save(storage, (proposal.status as u8, prop_id), &Empty {})?;
    IDX_PROPS_BY_PROPOSER.save(storage, (proposer.clone(), prop_id), &Empty {})?;

    Ok(())
}

fn create_deposit(
    storage: &mut dyn Storage,
    prop_id: u64,
    depositor: &Addr,
    amount: &Uint128,
) -> StdResult<()> {
    // deposit
    let deposit = DEPOSITS
        .may_load(storage, (prop_id, depositor.clone()))?
        .unwrap_or_default();
    if deposit.is_zero() {
        IDX_DEPOSITS_BY_DEPOSITOR.save(storage, (depositor.clone(), prop_id), &Empty {})?;
    }
    DEPOSITS.save(storage, (prop_id, depositor.clone()), &deposit.add(amount))?;

    Ok(())
}

fn update_proposal_status(
    storage: &mut dyn Storage,
    prop_id: u64,
    proposal: &mut Proposal,
    desired: Status,
) -> StdResult<()> {
    let before = proposal.status;
    proposal.status = desired;
    PROPOSALS.update(storage, prop_id, |prop| {
        if let Some(mut prop) = prop {
            prop.status = desired;
            Ok(prop)
        } else {
            Err(StdError::not_found("proposal"))
        }
    })?;
    IDX_PROPS_BY_STATUS.remove(storage, (before.clone() as u8, prop_id));
    IDX_PROPS_BY_STATUS.save(storage, (desired as u8, prop_id), &Empty {})?;

    Ok(())
}

fn proposal_deposit_refund_msgs(
    storage: &dyn Storage,
    prop_id: u64,
    gov_token: impl Into<String> + Clone,
    start_after: Option<Addr>,
    limit: Option<u32>,
) -> StdResult<Vec<CosmosMsg>> {
    let limit = get_and_check_limit(limit, 200, 100)? as usize;
    let order = Order::Ascending;
    let (min, max) = (start_after.map(Bound::<Addr>::exclusive), None);

    // refund all deposits. FIXME: add iteration logic to process refund
    let deposits: StdResult<Vec<CosmosMsg>> = DEPOSITS
        .prefix(prop_id)
        .range(storage, min, max, order)
        .take(limit)
        .map(|item| {
            let (depositor, amount) = item?;

            Ok(CosmosMsg::Bank(BankMsg::Send {
                to_address: depositor.to_string(),
                amount: coins(amount.u128(), gov_token.clone()),
            }))
        })
        .collect();

    deposits
}

pub fn propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    propose_msg: ProposeMsg,
) -> Result<Response, ContractError> {
    check_paused(deps.storage, &env.block)?;

    let cfg = CONFIG.load(deps.storage)?;
    let gov_token = GOV_TOKEN.load(deps.storage)?;

    let received = may_pay(&info, gov_token.as_str())
        .map_err(|e| ContractError::Std(StdError::generic_err(format!("{:?}", e))))?;
    if received < cfg.proposal_min_deposit {
        return Err(ContractError::Unauthorized {});
    }

    // Get total supply
    let total_supply = get_total_staked_supply(deps.as_ref())?;
    if total_supply.is_zero() {
        return Err(ContractError::LackOfStakes {});
    }

    // Create a proposal
    let mut prop = Proposal {
        // payload
        title: propose_msg.title,
        link: propose_msg.link,
        description: propose_msg.description,
        proposer: info.sender.clone(),
        msgs: propose_msg.msgs,
        status: Status::Pending,

        // time
        submitted_at: env.block.clone().into(),
        deposit_ends_at: duration_to_expiry(&env.block.clone().into(), &cfg.deposit_period),
        vote_starts_at: Default::default(),
        vote_ends_at: duration_to_expiry(
            &env.block.clone().into(),
            &cfg.deposit_period.add(cfg.voting_period)?,
        ), // set it to maximum

        // voting
        votes: Votes::default(),
        threshold: cfg.threshold,
        total_weight: total_supply,
        total_deposit: received, // initial deposit = received
        deposit_base_amount: cfg.proposal_deposit,
    };
    if received >= cfg.proposal_deposit {
        prop.activate_voting_period(env.block.clone().into(), &cfg.voting_period);
    }

    let id = next_id(deps.storage)?;
    create_deposit(deps.storage, id, &info.sender, &received)?;
    create_proposal(deps.storage, id, &info.sender, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "propose")
        .add_attribute("sender", info.sender)
        .add_attribute("status", format!("{:?}", prop.status))
        .add_attribute("deposit", received.to_string())
        .add_attribute("proposal_id", id.to_string()))
}

pub fn deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    prop_id: u64,
) -> Result<Response, ContractError> {
    check_paused(deps.storage, &env.block)?;

    let cfg = CONFIG.load(deps.storage)?;
    let gov_token = GOV_TOKEN.load(deps.storage)?;

    let received = may_pay(&info, gov_token.as_str())
        .map_err(|e| ContractError::Std(StdError::generic_err(format!("{:?}", e))))?;
    if received.is_zero() {
        return Err(ContractError::Unauthorized {});
    }

    let resp = Response::new()
        .add_attribute("action", "deposit")
        .add_attribute("denom", gov_token)
        .add_attribute("amount", received.to_string())
        .add_attribute("proposal_id", prop_id.to_string());

    let mut prop = PROPOSALS.load(deps.storage, prop_id)?;
    check_proposal_status(&prop, Status::Pending)?;
    if prop.deposit_ends_at.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    } else {
        create_deposit(deps.storage, prop_id, &info.sender, &received)?;

        prop.total_deposit += received;
        if prop.total_deposit >= cfg.proposal_deposit {
            // open
            update_proposal_status(deps.storage, prop_id, &mut prop, Status::Open)?;
            prop.activate_voting_period(env.block.into(), &cfg.voting_period);
            PROPOSALS.save(deps.storage, prop_id, &prop)?;
            Ok(resp.add_attribute("result", "open"))
        } else {
            // pending = prevent default
            PROPOSALS.save(deps.storage, prop_id, &prop)?;
            Ok(resp.add_attribute("result", "pending"))
        }
    }
}

pub fn vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    prop_id: u64,
    vote: Vote,
) -> Result<Response, ContractError> {
    check_paused(deps.storage, &env.block)?;

    // Ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, prop_id)?;
    check_proposal_status(&prop, Status::Open)?;
    if prop.vote_ends_at.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // Get voter balance at proposal start
    let vote_power = get_voting_power_at_height(
        deps.querier,
        STAKING_CONTRACT.load(deps.storage)?,
        info.sender.clone(),
        prop.vote_starts_at.height,
    )?;
    if vote_power.is_zero() {
        return Err(ContractError::Unauthorized {});
    }

    let ballot = BALLOTS.may_load(deps.storage, (prop_id, &info.sender))?;
    if let Some(ballot) = ballot {
        prop.votes.revoke(ballot.vote, ballot.weight);
    }
    prop.votes.submit(vote, vote_power);

    BALLOTS.save(
        deps.storage,
        (prop_id, &info.sender),
        &Ballot {
            weight: vote_power,
            vote,
        },
    )?;
    PROPOSALS.save(deps.storage, prop_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("vote", format!("{:?}", vote))
        .add_attribute("proposal_id", prop_id.to_string()))
}

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    prop_id: u64,
) -> Result<Response, ContractError> {
    check_paused(deps.storage, &env.block)?;

    let mut prop = PROPOSALS.load(deps.storage, prop_id)?;
    if !prop.vote_ends_at.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    prop.update_status(&env.block);
    check_proposal_status(&prop, Status::Passed)?;
    update_proposal_status(deps.storage, prop_id, &mut prop, Status::Executed)?;

    let gov_token = GOV_TOKEN.load(deps.storage)?;
    let refunds = proposal_deposit_refund_msgs(deps.storage, prop_id, gov_token, None, None)?;

    // Dispatch all proposed messages
    Ok(Response::new()
        .add_messages(refunds)
        .add_messages(prop.msgs)
        .add_attribute("action", "execute")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", prop_id.to_string()))
}

pub fn close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    prop_id: u64,
) -> Result<Response, ContractError> {
    check_paused(deps.storage, &env.block)?;

    let mut prop = PROPOSALS.load(deps.storage, prop_id)?;

    match prop.status {
        // * failed to satisfy minimum deposit amount -> confiscate
        Status::Pending => {
            if !prop.deposit_ends_at.is_expired(&env.block) {
                return Err(ContractError::NotExpired {});
            }
        }
        // * failed to pass vote threshold -> refund
        // * passed veto threshold -> confiscate
        Status::Open => {
            if !prop.vote_ends_at.is_expired(&env.block) {
                return Err(ContractError::NotExpired {});
            }
        }
        _ => {
            return Err(ContractError::InvalidProposalStatus {
                current: format!("{:?}", prop.status),
                desired: "pending | open".to_string(),
            })
        }
    }

    let prev_status = prop.status;
    prop.update_status(&env.block);
    check_proposal_status(&prop, Status::Rejected)?;
    update_proposal_status(deps.storage, prop_id, &mut prop, Status::Rejected)?;

    let mut resp = Response::new()
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender.to_string())
        .add_attribute("proposal_id", prop_id.to_string());

    if prev_status == Status::Open && !prop.is_vetoed() {
        // refund
        let gov_token = GOV_TOKEN.load(deps.storage)?;
        let refunds = proposal_deposit_refund_msgs(deps.storage, prop_id, gov_token, None, None)?;

        resp = resp.add_messages(refunds).add_attribute("result", "refund");
    } else {
        resp = resp.add_attribute("result", "confiscate")
    }

    Ok(resp)
}

pub fn pause_dao(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    expiration: Expiration,
) -> Result<Response, ContractError> {
    // Only contract can call this method
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    DAO_PAUSED.save(deps.storage, &expiration)?;

    Ok(Response::new()
        .add_attribute("action", "pause_dao")
        .add_attribute("expiration", expiration.to_string()))
}

pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    update_config_msg: Config,
) -> Result<Response, ContractError> {
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

pub fn update_staking_contract(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_staking_contract: Addr,
) -> Result<Response, ContractError> {
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

pub fn update_token_list(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    to_add: Vec<Denom>,
    to_remove: Vec<Denom>,
) -> Result<Response, ContractError> {
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

#[cfg(test)]
mod test {
    use cosmwasm_std::testing::MockStorage;

    use super::*;

    #[test]
    fn check_paused() {
        let mut storage = MockStorage::new();

        DAO_PAUSED
            .save(&mut storage, &Expiration::AtHeight(10))
            .unwrap();

        super::check_paused(
            &storage,
            &BlockInfo {
                height: 11,
                time: Default::default(),
                chain_id: "mock_chain".to_string(),
            },
        )
        .unwrap();

        let err = super::check_paused(
            &storage,
            &BlockInfo {
                height: 9,
                time: Default::default(),
                chain_id: "mock_chain".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Paused {})
    }

    #[test]
    fn check_proposal_status() {
        let make_prop = |status: Status| {
            let mut prop = Proposal::default();
            prop.status = status;
            prop
        };

        super::check_proposal_status(&make_prop(Status::Pending), Status::Pending).unwrap();

        let err =
            super::check_proposal_status(&make_prop(Status::Open), Status::Pending).unwrap_err();
        assert_eq!(
            err,
            ContractError::InvalidProposalStatus {
                current: "Open".to_string(),
                desired: "Pending".to_string()
            }
        )
    }

    #[test]
    fn create_proposal() {
        let mut storage = MockStorage::default();

        let proposer = Addr::unchecked("proposer");
        let proposal = Proposal::default();

        super::create_proposal(&mut storage, 1, &proposer, &proposal).unwrap();

        assert!(PROPOSALS.has(&storage, 1));
        assert!(IDX_PROPS_BY_STATUS.has(&storage, (Status::Pending as u8, 1)));
        assert!(IDX_PROPS_BY_PROPOSER.has(&storage, (proposer.clone(), 1)));
    }

    #[test]
    fn create_deposit() {
        let mut storage = MockStorage::default();

        let proposer = Addr::unchecked("proposer");
        let proposal = Proposal::default();

        super::create_proposal(&mut storage, 1, &proposer, &proposal).unwrap();

        let depositor = Addr::unchecked("depositor");

        // initial
        super::create_deposit(&mut storage, 1, &depositor, &Uint128::from(10u128)).unwrap();
        assert_eq!(
            DEPOSITS.load(&storage, (1, depositor.clone())).unwrap(),
            Uint128::from(10u128)
        );
        assert!(IDX_DEPOSITS_BY_DEPOSITOR.has(&storage, (depositor.clone(), 1)));

        super::create_deposit(&mut storage, 1, &depositor, &Uint128::from(10u128)).unwrap();
        assert_eq!(
            DEPOSITS.load(&storage, (1, depositor.clone())).unwrap(),
            Uint128::from(20u128)
        );
        assert!(IDX_DEPOSITS_BY_DEPOSITOR.has(&storage, (depositor.clone(), 1)));
    }

    #[test]
    fn update_proposal_status() {
        let mut storage = MockStorage::default();

        let proposer = Addr::unchecked("proposer");
        let mut proposal = Proposal::default();

        proposal.proposer = proposer.clone();
        super::create_proposal(&mut storage, 1, &proposer, &proposal).unwrap();

        proposal.proposer = Addr::unchecked("abuser");
        super::update_proposal_status(&mut storage, 1, &mut proposal, Status::Passed).unwrap();

        assert_eq!(PROPOSALS.load(&storage, 1).unwrap().status, Status::Passed);
        assert_eq!(PROPOSALS.load(&storage, 1).unwrap().proposer, proposer);
    }

    #[test]
    fn proposal_deposit_refund_msgs() {
        let mut storage = MockStorage::default();

        let proposer = Addr::unchecked("proposer");
        let mut proposal = Proposal::default();

        proposal.proposer = proposer.clone();
        super::create_proposal(&mut storage, 1, &proposer, &proposal).unwrap();

        let depositor1 = Addr::unchecked("depositor1");
        super::create_deposit(&mut storage, 1, &depositor1, &Uint128::from(10u128)).unwrap();
        let depositor2 = Addr::unchecked("depositor2");
        super::create_deposit(&mut storage, 1, &depositor2, &Uint128::from(20u128)).unwrap();
        let depositor3 = Addr::unchecked("depositor3");
        super::create_deposit(&mut storage, 1, &depositor3, &Uint128::from(30u128)).unwrap();

        let msgs =
            super::proposal_deposit_refund_msgs(&storage, 1, "gov_token", None, None).unwrap();
        let asserts: Vec<CosmosMsg> = vec![
            BankMsg::Send {
                to_address: "depositor1".to_string(),
                amount: coins(10u128, "gov_token"),
            }
            .into(),
            BankMsg::Send {
                to_address: "depositor2".to_string(),
                amount: coins(20u128, "gov_token"),
            }
            .into(),
            BankMsg::Send {
                to_address: "depositor3".to_string(),
                amount: coins(30u128, "gov_token"),
            }
            .into(),
        ];
        assert_eq!(msgs, asserts)
    }
}
