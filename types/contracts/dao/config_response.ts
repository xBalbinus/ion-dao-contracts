import { Addr, Config } from "./shared-types";

export interface ConfigResponse {
  config: Config;
  gov_token: string;
  staking_contract: Addr;
  [k: string]: unknown;
}