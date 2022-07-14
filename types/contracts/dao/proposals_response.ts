import {
  BlockTime,
  CosmosMsgFor_Empty,
  Decimal,
  Expiration,
  Status,
  Threshold,
  Uint128,
  Votes,
} from "./shared-types";

export interface ProposalsResponse {
  proposals: ProposalResponseFor_Empty[];
  [k: string]: unknown;
}
/**
 * Note, if you are storing custom messages in the proposal, the querier needs to know what possible custom message types those are in order to parse the response
 */
export interface ProposalResponseFor_Empty {
  deposit_ends_at: Expiration;
  description: string;
  id: number;
  link: string;
  msgs: CosmosMsgFor_Empty[];
  proposer: string;
  quorum: Decimal;
  status: Status;
  submitted_at: BlockTime;
  threshold: Threshold;
  title: string;
  total_deposit: Uint128;
  total_votes: Uint128;
  total_weight: Uint128;
  vote_ends_at: Expiration;
  vote_starts_at: BlockTime;
  votes: Votes;
  [k: string]: unknown;
}
