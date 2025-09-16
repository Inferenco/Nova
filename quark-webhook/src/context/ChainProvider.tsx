"use client";
import { getAptosClient } from "../aptos";
import { Aptos, Network } from "@aptos-labs/ts-sdk";
import {
  APTOS_API_KEY,
  APTOS_INDEXER,
  APTOS_NETWORK,
  APTOS_NODE_URL,
} from "../config/env";
import { createContext, useContext, useEffect, useState } from "react";
import {
  GasStationClient,
  GasStationTransactionSubmitter,
} from "@aptos-labs/gas-station-client";

type ChainProviderContextProp = {
  aptos: Aptos;
  createChainClient: () => void;
};

const ChainProviderContext = createContext<ChainProviderContextProp>(
  {} as ChainProviderContextProp
);

export const ChainProvider = ({ children }: { children: React.ReactNode }) => {
  const [aptos, setAptos] = useState<Aptos>({} as Aptos);

  useEffect(() => {
    createChainClient();
  }, []);

  const createChainClient = () => {
    const fullnode = APTOS_NODE_URL;
    const indexer = APTOS_INDEXER;

    const gasStationClient = new GasStationClient({
      network: APTOS_NETWORK === "mainnet" ? Network.MAINNET : Network.TESTNET,
      apiKey: APTOS_API_KEY,
    });

    const gasStationTransactionSubmitter = new GasStationTransactionSubmitter(
      gasStationClient
    );

    setAptos(
      getAptosClient(
        fullnode as string,
        indexer as string,
        gasStationTransactionSubmitter
      )
    );
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
