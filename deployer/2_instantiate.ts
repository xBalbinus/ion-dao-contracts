import { join } from "path";

import Config from "./config";
import { InstantiateResult, UploadResult } from "./types";
import { writeFileSync } from "fs";

import { dao, stake } from "../types/contracts";

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
          denom: "uion",
          label: "ION staking contract for governance",
          stake_contract_code_id: codes.ion_stake,
          unstaking_duration: { time: 3600 },
        },
        threshold: { quorum: "0.3", threshold: "0.5", veto_threshold: "0.3" },
        deposit_period: { time: 600 },
        voting_period: { time: 600 },
        proposal_deposit_amount: "100",
        proposal_deposit_min_amount: "50",
      } as dao.InitMsg,
      "ION governance contract",
      "auto"
    );

  console.log({ contractAddress: daoAddress, transactionHash });

  const configResp: dao.ConfigResponse = await cfg.cosmwasm.queryContractSmart(
    daoAddress,
    {
      get_config: {},
    } as dao.QueryMsg
  );

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
