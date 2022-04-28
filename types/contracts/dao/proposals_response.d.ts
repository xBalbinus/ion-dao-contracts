import { Addr, BlockTime, CosmosMsgFor_Empty, Decimal, Expiration, Status, Threshold, Uint128, Votes } from "./shared-types";

export interface ProposalsResponse {
proposals: ProposalResponseFor_Empty[]
[k: string]: unknown
}
/**
 * Note, if you are storing custom messages in the proposal, the querier needs to know what possible custom message types those are in order to parse the response
 */
export interface ProposalResponseFor_Empty {
deposit_amount: Uint128
deposit_starts_at: BlockTime
description: string
expires_at: Expiration
id: number
link: string
msgs: CosmosMsgFor_Empty[]
proposer: Addr
quorum: Decimal
status: Status
threshold: Threshold
title: string
total_votes: Uint128
total_weight: Uint128
vote_starts_at?: (BlockTime | null)
votes: Votes
[k: string]: unknown
}
