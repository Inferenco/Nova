"use client";
import { getAptosClient } from "../aptos";
import { Aptos, Network } from "@aptos-labs/ts-sdk";
import {
  APTOS_GAS_STATION_API_KEY,
  APTOS_INDEXER,
  APTOS_NETWORK,
  APTOS_NODE_URL,
} from "../config/env";
import { createContext, useContext, useEffect, useState } from "react";
import {
  GasStationTransactionSubmitter,
  GasStationClient,
} from "@aptos-labs/gas-station-client";

type ChainProviderContextProp = {
  aptos: Aptos | null;
  createChainClient: () => void;
};

const ChainProviderContext = createContext<ChainProviderContextProp>({
  aptos: null,
  createChainClient: () => {},
});

export const ChainProvider = ({ children }: { children: React.ReactNode }) => {
  const [aptos, setAptos] = useState<Aptos | null>(null);

  useEffect(() => {
    createChainClient();
  }, []);

  const createChainClient = () => {
    const fullnode = APTOS_NODE_URL;
    const indexer = APTOS_INDEXER;

    let gasStationTransactionSubmitter;
    try {
      if (!APTOS_GAS_STATION_API_KEY) {
        console.warn(
          "No gas station API key provided, transactions will not be sponsored"
        );
        gasStationTransactionSubmitter = undefined;
      } else {
        const gasStationClient = new GasStationClient({
          network:
            APTOS_NETWORK === "mainnet" ? Network.MAINNET : Network.TESTNET,
          apiKey: APTOS_GAS_STATION_API_KEY,
        });
        gasStationTransactionSubmitter = new GasStationTransactionSubmitter(
          gasStationClient
        );

        console.log("Gas Station Transaction Submitter created successfully");
      }
    } catch (error) {
      console.error("Error creating Gas Station Transaction Submitter:", error);
      gasStationTransactionSubmitter = undefined;
    }

    const aptosClient = getAptosClient(
      fullnode as string,
      indexer as string,
      gasStationTransactionSubmitter
    );
    setAptos(aptosClient);
  };

  return (
    <ChainProviderContext.Provider value={{ aptos, createChainClient }}>
      {children}
    </ChainProviderContext.Provider>
  );
};

export const useChain = () => {
  return useContext(ChainProviderContext);
};
