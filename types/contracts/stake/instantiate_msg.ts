import { Addr, Duration } from "./shared-types";

export interface InstantiateMsg {
  admin?: Addr | null;
  denom: string;
  unstaking_duration?: Duration | null;
  [k: string]: unknown;
}
