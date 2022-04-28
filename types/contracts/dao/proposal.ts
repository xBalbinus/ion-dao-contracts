import {
  Addr,
  BlockTime,
  CosmosMsgFor_Empty,
  Expiration,
  Status,
  Threshold,
  Uint128,
  Votes,
} from "./shared-types";

export interface Proposal {
  /**
   * Amount of the native governance token required for voting
   */
  deposit: Uint128;
  /**
   * Starting time / height information
   */
  deposit_starts_at: BlockTime;
  /**
   * Proposal Description
   */
  description: string;
  expires_at: Expiration;
  /**
   * Related link about this proposal
   */
  link: string;
  msgs: CosmosMsgFor_Empty[];
  /**
   * Address of proposer
   */
  proposer: Addr;
  status: Status;
  /**
   * Pass requirements
   */
  threshold: Threshold;
  /**
   * Proposal title
   */
  title: string;
  /**
   * The total weight when the proposal started (used to calculate percentages)
   */
  total_weight: Uint128;
  vote_starts_at?: BlockTime | null;
  /**
   * summary of existing votes
   */
  votes: Votes;
  [k: string]: unknown;
}
