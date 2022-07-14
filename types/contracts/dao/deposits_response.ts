import { DepositResponse } from "./shared-types";

export interface DepositsResponse {
  deposits: DepositResponse[];
  [k: string]: unknown;
}
