use cosmwasm_std::{Attribute, StdError, Uint128};
use cw3::Status;
use cw3::Vote;
use cw_utils::Expiration;

use crate::state::BlockTime;
use crate::tests::suite::SuiteBuilder;
use crate::ContractError;
use crate::CosmosMsg;

mod propose {
    use cosmwasm_std::{
        coin, coins, to_binary, BankMsg, DistributionMsg, GovMsg, IbcMsg, IbcTimeout, StakingMsg,
        VoteOption, WasmMsg,
    };
    use osmo_bindings::{OsmosisMsg, SwapAmountWithLimit};

    use super::*;

    fn assert_event_attrs(
        src: &[Attribute],
        sender: &str,
        status: Status,
        deposit: u128,
        proposal_id: u64,
    ) {
        assert_eq!(
            src,
            &[
                Attribute::new("action", "propose"),
                Attribute::new("sender", sender.to_string()),
                Attribute::new("status", format!("{:?}", status)),
                Attribute::new("deposit", deposit.to_string()),
                Attribute::new("proposal_id", proposal_id.to_string())
            ]
        )
    }

    #[test]
    fn should_work() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        let resp = suite
            .propose("tester0", "title", "link", "desc", vec![], Some(100u128))
            .unwrap();
        assert_event_attrs(resp.custom_attrs(1), "tester0", Status::Open, 100, 1);

        let prop = suite.query_proposal(1).unwrap();
        let block = suite.app().block_info();
        assert_eq!(prop.status, Status::Open);
        assert_eq!(
            prop.deposit_ends_at,
            Expiration::AtHeight(block.height + 15)
        );
        assert_eq!(prop.vote_starts_at, block.clone().into());
        assert_eq!(prop.vote_ends_at, Expiration::AtHeight(block.height + 10));
        assert_eq!(prop.total_weight, Uint128::new(100u128));
        assert_eq!(prop.total_deposit, Uint128::new(100u128));
    }

    #[test]
    fn should_work_with_min_deposit() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 10u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        let resp = suite
            .propose("tester0", "title", "link", "desc", vec![], Some(10u128))
            .unwrap();
        assert_event_attrs(resp.custom_attrs(1), "tester0", Status::Pending, 10, 1);

        let prop = suite.query_proposal(1).unwrap();
        let block = suite.app().block_info();
        assert_eq!(prop.status, Status::Pending);
        assert_eq!(
            prop.deposit_ends_at,
            Expiration::AtHeight(block.height + 15)
        );
        assert_eq!(prop.vote_starts_at, BlockTime::default());
        assert_eq!(prop.vote_ends_at, Expiration::AtHeight(block.height + 25));
        assert_eq!(prop.total_weight, Uint128::new(100u128));
        assert_eq!(prop.total_deposit, Uint128::new(10u128));
    }

    #[test]
    fn should_accept_various_msgs() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        let bank_msg = CosmosMsg::from(BankMsg::Send {
            to_address: "foo".to_string(),
            amount: coins(100u128, "bar"),
        });

        let staking_msg = CosmosMsg::from(StakingMsg::Delegate {
            validator: "foo".to_string(),
            amount: coin(100u128, "bar"),
        });

        let distribution_msg = CosmosMsg::from(DistributionMsg::SetWithdrawAddress {
            address: "foo".to_string(),
        });

        let stargate_msg = CosmosMsg::Stargate {
            type_url: "foo".to_string(),
            value: to_binary(&"bar").unwrap(),
        };

        let ibc_msg = CosmosMsg::from(IbcMsg::Transfer {
            channel_id: "foo".to_string(),
            to_address: "bar".to_string(),
            amount: coin(100u128, "foo"),
            timeout: IbcTimeout::with_timestamp(suite.app().block_info().time),
        });

        let wasm_msg = CosmosMsg::from(WasmMsg::Execute {
            contract_addr: "foo".to_string(),
            msg: to_binary(&"bar").unwrap(),
            funds: coins(100u128, "denom"),
        });

        let gov_msg = CosmosMsg::from(GovMsg::Vote {
            proposal_id: 0,
            vote: VoteOption::Yes,
        });

        let osmo_msg = CosmosMsg::from(OsmosisMsg::simple_swap(
            1,
            "foo",
            "bar",
            SwapAmountWithLimit::ExactIn {
                input: Uint128::new(100u128),
                min_output: Uint128::new(100u128),
            },
        ));

        let msgs = vec![
            bank_msg,
            staking_msg,
            distribution_msg,
            stargate_msg,
            ibc_msg,
            wasm_msg,
            gov_msg,
            osmo_msg,
        ];
        let resp = suite
            .propose(
                "tester0",
                "title",
                "link",
                "desc",
                msgs.clone(),
                Some(100u128),
            )
            .unwrap();
        assert_event_attrs(resp.custom_attrs(1), "tester0", Status::Open, 100, 1);

        let prop = suite.query_proposal(1).unwrap();
        assert_eq!(prop.msgs, msgs);
    }

    #[test]
    fn should_fail_if_not_enough_funds() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        let err = suite
            .propose("tester0", "title", "link", "desc", vec![], None)
            .unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());
    }

    #[test]
    fn should_fail_if_lack_of_stakes() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128)])
            .build();

        let err = suite
            .propose("tester0", "title", "link", "desc", vec![], Some(100u128))
            .unwrap_err();
        assert_eq!(ContractError::LackOfStakes {}, err.downcast().unwrap());
    }
}

mod deposit {
    use super::*;

    fn assert_event_attrs(src: &[Attribute], amount: u128, proposal_id: u64, result: &str) {
        assert_eq!(
            src,
            &[
                Attribute::new("action", "deposit"),
                Attribute::new("denom", "denom"),
                Attribute::new("amount", amount.to_string()),
                Attribute::new("proposal_id", proposal_id.to_string()),
                Attribute::new("result", result.to_string())
            ]
        )
    }

    #[test]
    fn should_work() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128), ("tester1", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        suite
            .propose("tester0", "title", "link", "desc", vec![], Some(10u128))
            .unwrap();

        let resp = suite.deposit("tester1", 1, Some(80u128)).unwrap();
        assert_event_attrs(resp.custom_attrs(1), 80, 1, "pending");

        let prop = suite.query_proposal(1).unwrap();
        assert_eq!(prop.status, Status::Pending);
        assert_eq!(prop.total_deposit, Uint128::new(90u128));

        let resp = suite.deposit("tester0", 1, Some(10u128)).unwrap();
        assert_event_attrs(resp.custom_attrs(1), 10, 1, "open");

        let prop = suite.query_proposal(1).unwrap();
        let block = suite.app().block_info();
        assert_eq!(prop.status, Status::Open);
        assert_eq!(prop.total_deposit, Uint128::new(100u128));
        assert_eq!(prop.vote_starts_at, block.clone().into());
        assert_eq!(prop.vote_ends_at, Expiration::AtHeight(block.height + 10));

        assert!(suite.check_balance("tester0", 80u128));
        assert!(suite.check_balance("tester1", 20u128));
    }

    #[test]
    fn should_fail_if_no_funds() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128), ("tester1", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        suite
            .propose("tester0", "title", "link", "desc", vec![], Some(100u128))
            .unwrap();

        let err = suite.deposit("tester1", 1, None).unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());
    }

    #[test]
    fn should_fail_if_no_proposal() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128), ("tester1", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        let err = suite.deposit("tester1", 1, Some(100u128)).unwrap_err();
        assert_eq!(
            ContractError::Std(StdError::not_found("ion_dao::proposal::Proposal")),
            err.downcast().unwrap()
        );
    }

    #[test]
    fn should_fail_if_status_is_invalid() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128), ("tester1", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        suite
            .propose("tester0", "title", "link", "desc", vec![], Some(100u128))
            .unwrap();

        let err = suite.deposit("tester1", 1, Some(100u128)).unwrap_err();
        assert_eq!(
            ContractError::InvalidProposalStatus {
                current: "Open".to_string(),
                desired: "Pending".to_string()
            },
            err.downcast().unwrap()
        );
    }
}

mod vote {
    use crate::state::Votes;

    use super::*;

    fn assert_event_attrs(src: &[Attribute], sender: &str, vote: Vote, proposal_id: u64) {
        assert_eq!(
            src,
            &[
                Attribute::new("action", "vote"),
                Attribute::new("sender", sender.to_string()),
                Attribute::new("vote", format!("{:?}", vote)),
                Attribute::new("proposal_id", proposal_id.to_string()),
            ]
        )
    }

    #[test]
    fn should_work() {
        let mut suite = SuiteBuilder::new()
            .with_staked(vec![
                ("tester0", 40u128),
                ("tester1", 30u128),
                ("tester2", 20u128),
                ("tester3", 10u128),
            ])
            .add_proposal("title", "link", "desc", vec![])
            .build();

        let prop = suite.query_proposal(1).unwrap();
        assert_eq!(prop.total_weight, Uint128::new(100u128));

        let mut votes = Votes::default();
        let mut total = 0u128;

        // initial vote
        let cases1 = [
            ("tester0", 40u128, Vote::No),
            ("tester1", 30u128, Vote::Yes),
            ("tester2", 20u128, Vote::Abstain),
            ("tester3", 10u128, Vote::Veto),
        ];

        for (voter, weight, vote) in cases1.iter() {
            let resp = suite.vote(voter, 1, *vote).unwrap();
            assert_event_attrs(resp.custom_attrs(1), voter, *vote, 1);

            total += weight;
            votes.submit(*vote, Uint128::new(*weight));

            let prop = suite.query_proposal(1).unwrap();
            assert_eq!(prop.status, Status::Open);
            assert_eq!(prop.total_votes, Uint128::new(total));
            assert_eq!(prop.votes, votes);
        }

        let votes_resp = suite.query_votes(1, None, None, None).unwrap();
        assert_eq!(
            votes_resp,
            crate::query::VotesResponse {
                votes: cases1
                    .map(|(voter, weight, vote)| crate::query::VoteInfo {
                        voter: voter.to_string(),
                        vote,
                        weight: Uint128::new(weight)
                    })
                    .to_vec()
            }
        );

        // override vote
        let cases2 = [
            ("tester0", 40u128, Vote::Veto),
            ("tester1", 30u128, Vote::Abstain),
            ("tester2", 20u128, Vote::Yes),
            ("tester3", 10u128, Vote::No),
        ];

        for (idx, (voter, weight, vote)) in cases2.iter().enumerate() {
            let resp = suite.vote(voter, 1, *vote).unwrap();
            assert_event_attrs(resp.custom_attrs(1), voter, *vote, 1);

            votes.revoke(cases1[idx].2, Uint128::new(cases1[idx].1));
            votes.submit(*vote, Uint128::new(*weight));

            let prop = suite.query_proposal(1).unwrap();
            assert_eq!(prop.status, Status::Open);
            assert_eq!(prop.total_votes, Uint128::new(total));
            assert_eq!(prop.votes, votes);
        }

        let votes_resp = suite.query_votes(1, None, None, None).unwrap();
        assert_eq!(
            votes_resp,
            crate::query::VotesResponse {
                votes: cases2
                    .map(|(voter, weight, vote)| crate::query::VoteInfo {
                        voter: voter.to_string(),
                        vote,
                        weight: Uint128::new(weight)
                    })
                    .to_vec()
            }
        );
    }

    #[test]
    fn should_fail_if_status_is_invalid() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 10u128)])
            .with_staked(vec![("tester0", 100u128)])
            .build();

        // make pending proposal
        suite
            .propose("tester0", "title", "link", "desc", vec![], Some(10u128))
            .unwrap();

        let err = suite.vote("tester0", 1, Vote::Yes).unwrap_err();
        assert_eq!(
            ContractError::InvalidProposalStatus {
                current: "Pending".to_string(),
                desired: "Open".to_string()
            },
            err.downcast().unwrap()
        );
    }

    #[test]
    fn should_fail_if_voting_period_expired() {
        let mut suite = SuiteBuilder::new()
            .with_staked(vec![("tester0", 100u128)])
            .add_proposal("title", "link", "desc", vec![])
            .build();

        suite.app().advance_blocks(10); // voting period

        let err = suite.vote("tester0", 1, Vote::Yes).unwrap_err();
        assert_eq!(ContractError::Expired {}, err.downcast().unwrap());
    }

    #[test]
    fn should_fail_if_no_voting_power() {
        let mut suite = SuiteBuilder::new()
            .with_staked(vec![("tester0", 100u128)])
            .add_proposal("title", "link", "desc", vec![])
            .build();

        let err = suite.vote("tester1", 1, Vote::Veto).unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());
    }
}

mod execute_proposal {
    use cosmwasm_std::{coins, Addr, BankMsg};
    use cw_multi_test::Executor;

    use super::*;

    fn assert_event_attrs(src: &[Attribute], sender: &str, proposal_id: u64) {
        assert_eq!(
            src,
            &[
                Attribute::new("action", "execute"),
                Attribute::new("sender", sender),
                Attribute::new("proposal_id", proposal_id.to_string())
            ]
        )
    }

    #[test]
    fn should_refund_deposit() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .add_proposal("title", "link", "desc", vec![])
            .build();

        suite.vote("tester0", 1, Vote::Yes).unwrap();
        suite.app().advance_blocks(10);

        let resp = suite.execute_proposal("owner", 1).unwrap();
        assert_event_attrs(resp.custom_attrs(1), "owner", 1);
        assert!(suite.check_balance("owner", 100u128));
    }

    #[test]
    fn should_execute_msgs() {
        let send_msg = CosmosMsg::from(BankMsg::Send {
            to_address: "tester0".to_string(),
            amount: coins(100u128, "denom"),
        });
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 100u128)])
            .with_staked(vec![("tester0", 100u128)])
            .add_proposal("title", "link", "desc", vec![send_msg])
            .build();

        let dao = suite.dao.clone();
        suite
            .app()
            .send_tokens(
                Addr::unchecked("tester0"),
                dao,
                coins(100u128, "denom").as_slice(),
            )
            .unwrap();
        suite.vote("tester0", 1, Vote::Yes).unwrap();
        suite.app().advance_blocks(10);

        let resp = suite.execute_proposal("owner", 1).unwrap();
        assert_event_attrs(resp.custom_attrs(1), "owner", 1);

        assert!(suite.check_balance("tester0", 100u128));
    }

    #[test]
    fn should_fail_if_voting_period_not_expired() {
        let mut suite = SuiteBuilder::new()
            .with_staked(vec![("tester0", 1u128)])
            .add_proposal("title", "link", "desc", vec![])
            .build();

        let err = suite.execute_proposal("owner", 1).unwrap_err();
        assert_eq!(ContractError::NotExpired {}, err.downcast().unwrap());
    }

    #[test]
    fn should_fail_if_status_is_invalid() {
        let mut suite = SuiteBuilder::new()
            .with_staked(vec![("tester0", 1u128)])
            .add_proposal("title", "link", "desc", vec![])
            .build();

        suite.vote("tester0", 1, Vote::No).unwrap();
        suite.app().advance_blocks(10);

        let err = suite.execute_proposal("owner", 1).unwrap_err();
        assert_eq!(
            ContractError::InvalidProposalStatus {
                current: "Rejected".to_string(),
                desired: "Passed".to_string()
            },
            err.downcast().unwrap()
        );
    }
}

mod close_proposal {
    use super::*;

    fn assert_event_attrs(src: &[Attribute], sender: &str, proposal_id: u64, result: &str) {
        assert_eq!(
            src,
            &[
                Attribute::new("action", "close"),
                Attribute::new("sender", sender),
                Attribute::new("proposal_id", proposal_id.to_string()),
                Attribute::new("result", result)
            ]
        )
    }

    #[test]
    fn should_refund_work() {
        let mut suite = SuiteBuilder::new()
            .with_staked(vec![("tester0", 70u128), ("tester1", 30u128)])
            .add_proposal("title", "link", "desc", vec![]) // 1
            .add_proposal("title", "link", "desc", vec![]) // 2
            .build();

        suite.vote("tester0", 1, Vote::No).unwrap();
        suite.vote("tester0", 2, Vote::Abstain).unwrap();
        suite.vote("tester1", 2, Vote::No).unwrap();
        suite.app().advance_blocks(10);

        let resp = suite.close_proposal("owner", 1).unwrap();
        assert_event_attrs(resp.custom_attrs(1), "owner", 1, "refund");
        assert!(suite.check_balance("owner", 100u128));

        let resp = suite.close_proposal("owner", 2).unwrap();
        assert_event_attrs(resp.custom_attrs(1), "owner", 2, "refund");
        assert!(suite.check_balance("owner", 200u128));
    }

    #[test]
    fn should_confiscate_work() {
        let mut suite = SuiteBuilder::new()
            .with_funds(vec![("tester0", 10u128)])
            .with_staked(vec![("tester0", 100u128)])
            .add_proposal("title", "link", "desc", vec![])
            .build();
        // min deposit not satisfied
        suite
            .propose("tester0", "title", "link", "desc", vec![], Some(10u128))
            .unwrap();
        // vetoed
        suite.vote("tester0", 1, Vote::Veto).unwrap();

        suite.app().advance_blocks(15);

        let resp = suite.close_proposal("owner", 1).unwrap();
        assert_event_attrs(resp.custom_attrs(1), "owner", 1, "confiscate");
        assert!(suite.check_balance("owner", 0u128));

        let resp = suite.close_proposal("owner", 2).unwrap();
        assert_event_attrs(resp.custom_attrs(1), "owner", 2, "confiscate");
        assert!(suite.check_balance("tester0", 0u128));
    }

    #[test]
    fn should_fail_if_status_is_invalid() {
        let mut suite = SuiteBuilder::new()
            .with_staked(vec![("tester0", 50u128)])
            .add_proposal("title", "link", "desc", vec![])
            .build();

        suite.vote("tester0", 1, Vote::Yes).unwrap();
        suite.app().advance_blocks(10);

        suite.execute_proposal("owner", 1).unwrap();

        let err = suite.close_proposal("abuser", 1).unwrap_err();
        assert_eq!(
            ContractError::InvalidProposalStatus {
                current: "Executed".to_string(),
                desired: "pending | open".to_string()
            },
            err.downcast().unwrap()
        );
    }

    #[test]
    fn should_fail_if_close_passed_proposal() {
        let mut suite = SuiteBuilder::new()
            .with_staked(vec![("tester0", 50u128)])
            .add_proposal("title", "link", "desc", vec![])
            .build();

        suite.vote("tester0", 1, Vote::Yes).unwrap();
        suite.app().advance_blocks(10);

        let err = suite.close_proposal("abuser", 1).unwrap_err();
        assert_eq!(
            ContractError::InvalidProposalStatus {
                current: "Passed".to_string(),
                desired: "Rejected".to_string()
            },
            err.downcast().unwrap()
        )
    }
}
