import { Denom } from "./shared-types";

export type QueryMsg = ({
threshold: {
[k: string]: unknown
}
} | {
proposal: {
proposal_id: number
[k: string]: unknown
}
} | {
list_proposals: {
limit?: (number | null)
start_after?: (number | null)
[k: string]: unknown
}
} | {
reverse_proposals: {
limit?: (number | null)
start_before?: (number | null)
[k: string]: unknown
}
} | {
proposal_count: {
[k: string]: unknown
}
} | {
vote: {
proposal_id: number
voter: string
[k: string]: unknown
}
} | {
list_votes: {
limit?: (number | null)
proposal_id: number
start_after?: (string | null)
[k: string]: unknown
}
} | {
tally: {
proposal_id: number
[k: string]: unknown
}
} | {
voter: {
address: string
[k: string]: unknown
}
} | {
get_config: {
[k: string]: unknown
}
} | {
balances: {
asset_type?: (Denom | null)
limit?: (number | null)
start_after?: (string | null)
[k: string]: unknown
}
} | {
token_list: {
[k: string]: unknown
}
})
