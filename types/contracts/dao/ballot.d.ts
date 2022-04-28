import { Uint128, Vote } from "./shared-types";

export interface Ballot {
vote: Vote
weight: Uint128
[k: string]: unknown
}
