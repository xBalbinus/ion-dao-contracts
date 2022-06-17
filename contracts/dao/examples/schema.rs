use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema_with_title, remove_schemas, schema_for};

use ion_dao::msg;
use ion_dao::state;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema_with_title(&schema_for!(msg::InstantiateMsg), &out_dir, "InitMsg");
    export_schema_with_title(&schema_for!(msg::ExecuteMsg), &out_dir, "ExecuteMsg");
    export_schema_with_title(&schema_for!(msg::QueryMsg), &out_dir, "QueryMsg");

    export_schema_with_title(&schema_for!(state::Config), &out_dir, "Config");
    export_schema_with_title(&schema_for!(state::Proposal), &out_dir, "Proposal");
    export_schema_with_title(&schema_for!(state::BlockTime), &out_dir, "BlockTime");
    export_schema_with_title(&schema_for!(state::Ballot), &out_dir, "Ballot");
    export_schema_with_title(&schema_for!(state::Votes), &out_dir, "Votes");
    export_schema_with_title(&schema_for!(state::Threshold), &out_dir, "Threshold");

    export_schema_with_title(
        &schema_for!(msg::ConfigResponse),
        &out_dir,
        "ConfigResponse",
    );
    export_schema_with_title(
        &schema_for!(msg::TokenListResponse),
        &out_dir,
        "TokenListResponse",
    );
    export_schema_with_title(
        &schema_for!(msg::TokenBalancesResponse),
        &out_dir,
        "TokenBalancesResponse",
    );

    export_schema_with_title(
        &schema_for!(msg::ProposalResponse),
        &out_dir,
        "ProposalResponse",
    );
    export_schema_with_title(
        &schema_for!(msg::ProposalsResponse),
        &out_dir,
        "ProposalsResponse",
    );

    export_schema_with_title(&schema_for!(msg::VoteInfo), &out_dir, "VoteInfo");
    export_schema_with_title(&schema_for!(msg::VoteResponse), &out_dir, "VoteResponse");
    export_schema_with_title(&schema_for!(msg::VotesResponse), &out_dir, "VotesResponse");

    export_schema_with_title(
        &schema_for!(msg::DepositResponse),
        &out_dir,
        "DepositResponse",
    );
    export_schema_with_title(
        &schema_for!(msg::DepositsResponse),
        &out_dir,
        "DepositsResponse",
    );
}
