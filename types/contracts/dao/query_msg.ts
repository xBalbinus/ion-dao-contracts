import {Addr, Denom, Status} from "./shared-types";

export type QueryMsg =
  | {
      get_config: {
        [k: string]: unknown;
      };
    }
  | {
      token_list: {
        [k: string]: unknown;
      };
    }
  | {
      token_balances: {
        limit?: number | null;
        order?: RangeOrder | null;
        start?: Denom | null;
        [k: string]: unknown;
      };
    }
  | {
      proposal: {
        proposal_id: number;
        [k: string]: unknown;
      };
    }
  | {
      proposals: {
        limit?: number | null;
        order?: RangeOrder | null;
        query: ProposalsQueryOption;
        start?: number | null;
        [k: string]: unknown;
      };
    }
  | {
      proposal_count: {
        [k: string]: unknown;
      };
    }
  | {
      vote: {
        proposal_id: number;
        voter: string;
        [k: string]: unknown;
      };
    }
  | {
      votes: {
        limit?: number | null;
        order?: RangeOrder | null;
        proposal_id: number;
        start?: string | null;
        [k: string]: unknown;
      };
    };
export type RangeOrder = "asc" | "desc";
export type ProposalsQueryOption =
  | {
      findbystatus: {
        status: Status;
        [k: string]: unknown;
      };
    }
  | {
      findbyproposer: {
        proposer: Addr;
        [k: string]: unknown;
      };
    }
  | {
      everything: {
        [k: string]: unknown;
      };
    };
