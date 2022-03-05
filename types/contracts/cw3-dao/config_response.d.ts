import { Addr, Config, Denom } from "./shared-types";

export interface ConfigResponse {
config: Config
gov_token: Denom
staking_contract: Addr
[k: string]: unknown
}
