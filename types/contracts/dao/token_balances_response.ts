import { Addr, Coin, Uint128 } from "./shared-types";

export type Balance =
  | {
      native: NativeBalance;
    }
  | {
      cw20: Cw20CoinVerified;
    };
export type NativeBalance = Coin[];

export interface TokenBalancesResponse {
  balances: Balance[];
  [k: string]: unknown;
}

export interface Cw20CoinVerified {
  address: Addr;
  amount: Uint128;
  [k: string]: unknown;
}
