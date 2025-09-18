"use client";

import { AptosWalletAdapterProvider } from "@aptos-labs/wallet-adapter-react";
import { PropsWithChildren } from "react";
import { AptosConfig, Network } from "@aptos-labs/ts-sdk";
import { APTOS_INDEXER, APTOS_NETWORK, APTOS_NODE_URL } from "../config/env";
import { useChain } from "./ChainProvider";

export const WalletProvider = ({ children }: PropsWithChildren) => {
  const { aptos } = useChain();

  // Don't render until aptos is ready
  if (!aptos?.config) {
    return <div>Loading...</div>;
  }

  return (
    <AptosWalletAdapterProvider
      dappConfig={{
        ...aptos.config,
        aptosConnect: {
          dappName: "Nova",
        },
      }}
      onError={(error) => {
        console.error("Error in wallet adapter:", error);
      }}
      optInWallets={[
        "Continue with Google",
        "Continue with Apple",
        "Petra",
        "Pontem Wallet",
        "Nightly",
        "OKX Wallet",
      ]}
      autoConnect
    >
      {children}
    </AptosWalletAdapterProvider>
  );
};
