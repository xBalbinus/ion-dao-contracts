import {Addr, BlockTime, CosmosMsgFor_OsmosisMsg, Expiration, Status, Threshold, Uint128, Votes,} from "./shared-types";

export interface Proposal {
  deposit_base_amount: Uint128;
  deposit_ends_at: Expiration;
  /**
   * Proposal Description
   */
  description: string;
  /**
   * Related link about this proposal
   */
  link: string;
  /**
   * List of messages to execute
   */
  msgs: CosmosMsgFor_OsmosisMsg[];
  /**
   * Address of proposer
   */
  proposer: Addr;
  /**
   * Current status of this proposal
   */
  status: Status;
  /**
   * Starting time / height information
   */
  submitted_at: BlockTime;
  /**
   * Pass requirements
   */
  threshold: Threshold;
  /**
   * Proposal title
   */
  title: string;
  /**
   * Amount of the native governance token required for voting
   */
  total_deposit: Uint128;
  /**
   * The total weight when the proposal started (used to calculate percentages)
   */
  total_weight: Uint128;
  vote_ends_at: Expiration;
  vote_starts_at: BlockTime;
  /**
   * summary of existing votes
   */
  votes: Votes;
  [k: string]: unknown;
}
