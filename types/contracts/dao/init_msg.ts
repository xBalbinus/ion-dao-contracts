import { Duration, Threshold, Uint128 } from "./shared-types";

export interface InitMsg {
  deposit_period: Duration;
  description: string;
  /**
   * Set an existing governance token or launch a new one
   */
  gov_token: GovToken;
  name: string;
  /**
   * Deposit required to make a proposal
   */
  proposal_deposit_amount: Uint128;
  /**
   * Voting params configuration
   */
  threshold: Threshold;
  voting_period: Duration;
  [k: string]: unknown;
}
export interface GovToken {
  denom: string;
  label: string;
  stake_contract_code_id: number;
  unstaking_duration?: Duration | null;
  [k: string]: unknown;
}
