import { Denom } from "./shared-types";

export interface TokenListResponse {
  token_list: Denom[];
  [k: string]: unknown;
}
