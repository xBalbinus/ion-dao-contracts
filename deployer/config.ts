import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import {
  DirectSecp256k1HdWallet,
  OfflineDirectSigner,
} from "@cosmjs/proto-signing";
import { GasPrice, SigningStargateClient } from "@cosmjs/stargate";

interface ConfigOption {
  mnemonic?: string;
  endpoint?: string;
  gasPrice?: GasPrice;
}

export default class Config {
  protected constructor(
    public readonly wallet: OfflineDirectSigner,
    public readonly cosmwasm: SigningCosmWasmClient,
    public readonly stargate: SigningStargateClient
  ) {}

  static new = async ({
    mnemonic = process.env.MNEMONIC || "",
    endpoint = process.env.ENDPOINT || "https://testnet-rpc.osmosis.zone/",
    gasPrice = GasPrice.fromString((process.env.GAS_PRICE = "0.015uosmo")),
  }: ConfigOption = {}): Promise<Config> => {
    if (mnemonic === "") {
      throw Error("please setup mnemonic phrase ($MNEMONIC)");
    }

    const wallet = await DirectSecp256k1HdWallet.fromMnemonic(mnemonic, {
      prefix: "osmo",
    });
    const cosmwasm = await SigningCosmWasmClient.connectWithSigner(
      endpoint,
      wallet,
      { gasPrice }
    );
    const stargate = await SigningStargateClient.connectWithSigner(
      endpoint,
      wallet,
      { gasPrice }
    );

    return new Config(wallet, cosmwasm, stargate);
  };
}
