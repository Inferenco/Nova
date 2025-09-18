"use client";
import { useWallet } from "@aptos-labs/wallet-adapter-react";
import { AgentRuntime, WalletSigner } from "move-agent-kit-fullstack";
import { createContext, useContext, useEffect, useState } from "react";
import {
  APTOS_GAS_STATION_API_KEY,
  PANORA_API_KEY,
  APTOS_NETWORK,
} from "../config/env";
import { Account, Network } from "@aptos-labs/ts-sdk";
import { useChain } from "./ChainProvider";
import { getAptosClient } from "../aptos";
import {
  GasStationTransactionSubmitter,
  GasStationClient,
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
    if (!aptos?.config) return;

    const signer = new WalletSigner({} as Account, wallet);
    let gasStationTransactionSubmitter;
    try {
      if (!APTOS_GAS_STATION_API_KEY) {
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
        console.log(
          "Agent Gas Station Transaction Submitter created successfully"
        );
      }
    } catch (error) {
      console.error(
        "Error creating Agent Gas Station Transaction Submitter:",
        error
      );
      gasStationTransactionSubmitter = undefined;
    }

    const aptosConfig = getAptosClient(
      aptos.config.fullnode as string,
      aptos.config.indexer as string,
      gasStationTransactionSubmitter,
      APTOS_NETWORK === "mainnet" ? Network.MAINNET : Network.TESTNET
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
