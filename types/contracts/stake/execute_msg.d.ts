import { Addr, Duration, Uint128 } from "./shared-types";

export type ExecuteMsg = ({
stake: {
[k: string]: unknown
}
} | {
unstake: {
amount: Uint128
[k: string]: unknown
}
} | {
fund: {
[k: string]: unknown
}
} | {
claim: {
[k: string]: unknown
}
} | {
update_config: {
admin?: (Addr | null)
duration?: (Duration | null)
[k: string]: unknown
}
})
