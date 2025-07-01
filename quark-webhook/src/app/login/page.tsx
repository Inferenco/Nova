"use client";

import { QuarkUserAbi } from "@/aptos";
import { useAbiClient } from "@/context/AbiProvider";
import { useWallet } from "@aptos-labs/wallet-adapter-react";
import { sendData } from "@telegram-apps/sdk-react";
import { useEffect, useState } from "react";
import { useWalletClient } from "@thalalabs/surf/hooks";
import { ACCOUNT_SEED, EXPLORER_URL } from "@/config/env";
import { AccountStatus } from "./components/AccountStatus";
import { Message } from "@/components/Message/Message";
import { useMessage } from "@/hooks/useMessage";
import { Section, Button } from "@telegram-apps/telegram-ui";
import { useSearchParams } from "next/navigation";
import { useActionDelay } from "@/hooks/useActionDelay";

const LoginPage = () => {
  const { abi } = useAbiClient();
  const { account } = useWallet();
  const { client } = useWalletClient();
  const searchParams = useSearchParams();
  const [resourceAccount, setResourceAccount] = useState<string>("");
  const [txHash, setTxHash] = useState<string>();
  const [isLoading, setIsLoading] = useState(false);
  const { message, showMessage } = useMessage();
  const { isDelaying, delayAction } = useActionDelay(1500);

  // Get user ID from URL parameters
  const userId = searchParams.get("userId") || searchParams.get("user_id");

  const handleLogin = async () => {
    if (!userId || !abi || !account?.address) {
      showMessage("Missing required data for login", "error");
      return;
    }

    setIsLoading(true);

    try {
      const userIdNum = parseInt(userId);

      const resourceAccount = await abi
        ?.useABI(QuarkUserAbi)
        .view.exists_resource_account({
          typeArguments: [],
          functionArguments: [account?.address.toString() as `0x${string}`],
        });

      if (!resourceAccount?.[0]) {
        const seed = ACCOUNT_SEED;
        const tx = await client?.useABI(QuarkUserAbi).create_account({
          type_arguments: [],
          arguments: [`${userIdNum.toString()}_${seed}`],
        });

        showMessage(
          "Account created successfully! Check the transaction details below.",
          "success"
        );

        setTxHash(tx?.hash);
      }

      const resourceAccountResponse = await abi
        ?.useABI(QuarkUserAbi)
        .view.get_resource_account({
          typeArguments: [],
          functionArguments: [account?.address.toString() as `0x${string}`],
        });

      const resourceAccountAddress = resourceAccountResponse?.[0] as string;

      showMessage(
        "Account created successfully! Check the transaction details below.",
        "success"
      );

      setResourceAccount(resourceAccountAddress);

      await delayAction(3000);

      if (sendData.isAvailable()) {
        sendData(
          JSON.stringify({
            accountAddress: account?.address.toString(),
            resourceAccountAddress: resourceAccountAddress,
          })
        );
      }
    } catch (error) {
      console.error("Login error:", error);
      showMessage("Login failed. Please try again.", "error");
    } finally {
      setIsLoading(false);
    }
  };

  const isReadyToLogin = userId && abi && account?.address;

  return (
    <Section className="min-h-screen">
      <Message message={message} />

      <div className="p-4 space-y-4">
        <div className="text-center">
          <h1
            className="text-2xl font-bold mb-2"
            style={{ color: "var(--tg-theme-text-color)" }}
          >
            Quark Login
          </h1>
          <p
            className="text-sm mb-4"
            style={{ color: "var(--tg-theme-hint-color)" }}
          >
            Connect your wallet and create your Quark account
            <span
              className="text-sm"
              style={{ color: "var(--tg-theme-hint-color)" }}
            >
              SEED: {ACCOUNT_SEED}
            </span>
          </p>
        </div>

        {isReadyToLogin ? (
          <div className="flex justify-center">
            <Button
              onClick={handleLogin}
              disabled={isLoading || isDelaying}
              className="px-8 py-3 text-lg font-semibold"
              style={{
                backgroundColor:
                  isLoading || isDelaying
                    ? "var(--tg-theme-secondary-bg-color)"
                    : "var(--tg-theme-button-color)",
                color:
                  isLoading || isDelaying
                    ? "var(--tg-theme-hint-color)"
                    : "var(--tg-theme-button-text-color)",
                borderRadius: "12px",
                minWidth: "200px",
              }}
            >
              {isLoading
                ? "Creating Account..."
                : isDelaying
                ? "Processing..."
                : "Login to Quark"}
            </Button>
          </div>
        ) : (
          <div className="text-center">
            <p
              className="text-sm"
              style={{ color: "var(--tg-theme-hint-color)" }}
            >
              {!account?.address
                ? "Please connect your wallet first"
                : "Loading..."}
            </p>
          </div>
        )}
      </div>

      <AccountStatus txHash={txHash} resourceAddress={resourceAccount} />
    </Section>
  );
};

export default LoginPage;
