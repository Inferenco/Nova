"use client";
import { useWallet } from "@aptos-labs/wallet-adapter-react";
import { AgentRuntime, WalletSigner } from "move-agent-kit-fullstack";
import { createContext, useContext, useEffect, useState } from "react";
import { APTOS_API_KEY, PANORA_API_KEY } from "../config/env";
import { Account, Network } from "@aptos-labs/ts-sdk";
import { useChain } from "./ChainProvider";
import { getAptosClient } from "../aptos";
import {
  GasStationClient,
  GasStationTransactionSubmitter,
} from "@aptos-labs/gas-station-client";

type Agent = {
  agent: AgentRuntime;
};

const AgentContext = createContext<Agent>({} as Agent);

export const AgentProvider = ({ children }: { children: React.ReactNode }) => {
  const [agent, setAgent] = useState<AgentRuntime>({} as AgentRuntime);
  const wallet = useWallet();
  const { aptos } = useChain();

  useEffect(() => {
    if (!aptos) return;

    const signer = new WalletSigner({} as Account, wallet);
    const gasStationClient = new GasStationClient({
      network: Network.MAINNET,
      apiKey: APTOS_API_KEY,
    });
    const gasStationTransactionSubmitter = new GasStationTransactionSubmitter(
      gasStationClient
    );

    const aptosConfig = getAptosClient(
      aptos.config.fullnode as string,
      aptos.config.indexer as string,
      gasStationTransactionSubmitter,
      Network.MAINNET
    );

    const agentInstance = new AgentRuntime(signer, aptosConfig, {
      PANORA_API_KEY,
    });

    setAgent(agentInstance);
  }, [wallet, aptos]);

  const values = { agent };

  return (
    <AgentContext.Provider value={values}>{children}</AgentContext.Provider>
  );
};

export const useAgent = () => {
  return useContext(AgentContext);
};
