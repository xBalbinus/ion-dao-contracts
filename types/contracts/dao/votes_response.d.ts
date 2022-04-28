import { VoteInfo } from "./shared-types";

export interface VotesResponse {
votes: VoteInfo[]
[k: string]: unknown
}
