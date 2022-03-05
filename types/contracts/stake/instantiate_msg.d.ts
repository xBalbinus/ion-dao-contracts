import { Addr, Denom, Duration } from "./shared-types";

export interface InstantiateMsg {
admin?: (Addr | null)
asset: Denom
unstaking_duration?: (Duration | null)
[k: string]: unknown
}
