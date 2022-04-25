import { join } from "path";

import Config from "./config";
import { InstantiateResult, UploadResult } from "./types";
import * as daoType from "../types/contracts/dao";
import * as stakeType from "../types/contracts/stake";
import { writeFileSync } from "fs";

async function main() {
  const cfg = await Config.new();
  const [account] = await cfg.wallet.getAccounts();

  console.log(account.address);
  console.log(
    await Promise.all([
      cfg.cosmwasm.getBalance(account.address, "uosmo"),
      cfg.cosmwasm.getBalance(account.address, "uion"),
    ])
  );

  const codes = require(join(__dirname, "1_upload")) as UploadResult;

  // instantiate
  const { contractAddress: daoAddress, transactionHash } =
    await cfg.cosmwasm.instantiate(
      account.address,
      codes.ion_dao,
      {
        name: "ION DAO",
        description: "DAO of the ION holders",
        gov_token: {
          use_native: {
            denom: "uion",
            label: "ION staking contract for governance",
            stake_contract_code_id: codes.ion_stake,
            unstaking_duration: { height: 10 },
          },
        },
        threshold: {
          threshold_quorum: { quorum: "0.3", threshold: "0.5" },
        },
        max_voting_period: { height: 50 },
        proposal_deposit_amount: "100",
      } as daoType.InstantiateMsg,
      "ION governance contract",
      "auto"
    );

  console.log({ contractAddress: daoAddress, transactionHash });

  const configResp: daoType.ConfigResponse =
    await cfg.cosmwasm.queryContractSmart(daoAddress, {
      get_config: {},
    } as daoType.QueryMsg);

  const stakeAddress = configResp.staking_contract;

  writeFileSync(
    join(__dirname, "2_instantiate.json"),
    JSON.stringify(
      {
        ion_dao: daoAddress,
        ion_stake: stakeAddress,
      } as InstantiateResult,
      null,
      2
    )
  );
}

main()
  .then(() => console.log("Done"))
  .catch(console.error);
