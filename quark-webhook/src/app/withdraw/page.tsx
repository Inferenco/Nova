"use client";

import { useCallback, useEffect, useState } from "react";
import { useAbiClient } from "@/context/AbiProvider";
import { QuarkUserAbi } from "@/aptos";
import { useWalletClient } from "@thalalabs/surf/hooks";
import { ICoin } from "@/helpers";
import { useChain } from "@/context/ChainProvider";
import { useWallet } from "@aptos-labs/wallet-adapter-react";
import {
  Section,
  Cell,
  List,
  Button,
  Input,
  Card,
  Title,
  Text,
  Caption,
  Avatar,
  Badge,
} from "@telegram-apps/telegram-ui";
import { Page } from "@/components/Page";
import { Message } from "@/components/Message/Message";
import { useMessage } from "@/hooks/useMessage";
import { useActionDelay } from "@/hooks/useActionDelay";
import {
  convertAmountFromOnChainToHumanReadable,
  AccountAddress,
} from "@aptos-labs/ts-sdk";
import { useSearchParams } from "next/navigation";
import { useLaunchParams, closeMiniApp } from "@telegram-apps/sdk-react";

export default function WithdrawPage() {
  const { account, connected, connect } = useWallet();
  const { abi } = useAbiClient();
  const { aptos } = useChain();
  const [coins, setCoins] = useState<ICoin[]>([]);
  const [selectedCoin, setSelectedCoin] = useState<ICoin | null>(null);
  const [amount, setAmount] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [withdrawing, setWithdrawing] = useState(false);
  const { message, showMessage } = useMessage();
  const { isDelaying, delayAction } = useActionDelay(1500);
  const { client } = useWalletClient();
  const searchParams = useSearchParams();

  // Get query parameters for coin and amount
  const coinParam = searchParams.get("coin"); // asset_type or symbol
  const amountParam = searchParams.get("amount");
  const launchParams = useLaunchParams();

  useEffect(() => {
    if (!account?.address || !aptos || !connected) return;

    console.log(launchParams);

    fetchCoins();
  }, [aptos, account?.address, abi, connected]);

  // Handle query parameters when coins are loaded
  useEffect(() => {
    if (!coinParam || coins.length === 0) return;

    // Find coin by asset_type (exact match) or symbol (case-insensitive)
    const targetCoin = coins.find(
      (coin) =>
        coin.asset_type === coinParam ||
        getCoinSymbol(coin).toLowerCase() === coinParam.toLowerCase()
    );

    if (targetCoin) {
      setSelectedCoin(targetCoin);

      // Set amount if provided and valid
      if (amountParam) {
        const parsedAmount = parseFloat(amountParam);
        if (!isNaN(parsedAmount) && parsedAmount > 0) {
          setAmount(parsedAmount.toString());
        }
      }
    } else {
      // Show message if coin not found
      showMessage(
        `Token "${coinParam}" not found or has zero balance`,
        "error"
      );
    }
  }, [coins, coinParam, amountParam]);

  const fetchCoins = useCallback(async () => {
    try {
      setLoading(true);

      const resourceAccountResponse = await abi
        ?.useABI(QuarkUserAbi)
        .view.get_resource_account({
          typeArguments: [],
          functionArguments: [account?.address.toString() as `0x${string}`],
        });

      let resourceAccount = resourceAccountResponse?.[0];

      if (!resourceAccount) {
        showMessage("Resource account not found", "error");
        return;
      }

      let resourceAccountAddress = AccountAddress.fromString(resourceAccount);

      if (!aptos) {
        showMessage("Aptos client not found", "error");
        return;
      }

      const coinsData = await aptos.getAccountCoinsData({
        accountAddress: resourceAccountAddress,
      });

      // Filter coins with non-zero amounts
      const nonZeroCoins = coinsData.filter((coin) => {
        const amount = coin.amount;
        return amount && parseFloat(amount.toString()) > 0;
      });

      setCoins(nonZeroCoins);
    } catch (error) {
      console.error("Error fetching coins:", error);
      showMessage("Failed to fetch coins", "error");
    } finally {
      setLoading(false);
    }
  }, [aptos, account?.address]);

  const formatCoinAmount = (coin: ICoin): string => {
    if (!coin.amount || !coin.metadata?.decimals) return "0";

    const amount = parseFloat(coin.amount.toString());
    const decimals = coin.metadata.decimals;
    return convertAmountFromOnChainToHumanReadable(amount, decimals).toString();
  };

  const getCoinSymbol = (coin: ICoin): string => {
    return coin.metadata?.symbol || "Unknown";
  };

  const getCoinName = (coin: ICoin): string => {
    return coin.metadata?.name || "Unknown Token";
  };

  const getTokenStandard = (coin: ICoin): string => {
    return coin.token_standard || coin.metadata?.token_standard || "v2";
  };

  const handleWithdraw = async () => {
    // Prevent execution if already processing or delaying
    if (withdrawing || isDelaying) {
      return;
    }

    if (!selectedCoin || !amount || !client) {
      // Use delay to prevent rapid successive attempts
      showMessage("Please select a coin and enter amount", "error");
      await delayAction(1500);
      return;
    }

    const withdrawAmount = parseFloat(amount);
    if (withdrawAmount <= 0) {
      showMessage("Amount must be greater than 0", "error");
      await delayAction(1500);
      return;
    }

    const maxAmount = parseFloat(
      formatCoinAmount(selectedCoin).replace(/,/g, "")
    );
    if (withdrawAmount > maxAmount) {
      showMessage("Amount exceeds available balance", "error");
      await delayAction(1500);
      return;
    }

    try {
      setWithdrawing(true);

      const tokenStandard = getTokenStandard(selectedCoin);
      const decimals = selectedCoin.metadata?.decimals || 8;
      const rawAmount = Math.floor(withdrawAmount * Math.pow(10, decimals));

      let response;

      if (tokenStandard === "v1") {
        // Use handleWithdrawFundsV1 for v1 tokens
        response = await client.useABI(QuarkUserAbi).withdraw_funds_v1({
          type_arguments: [selectedCoin.asset_type!],
          arguments: [rawAmount],
        });
      } else {
        // Use handleWithdrawFundsV2 for v2 tokens
        response = await client.useABI(QuarkUserAbi).withdraw_funds_v2({
          type_arguments: [],
          arguments: [rawAmount, selectedCoin.asset_type as `0x${string}`],
        });
      }

      if (response) {
        // Show success message with delay before allowing next action
        showMessage(
          `Successfully withdrew ${amount} ${getCoinSymbol(selectedCoin)}`,
          "success"
        );
        await delayAction(2000);
        setAmount("");
        setSelectedCoin(null);
        // Refresh coins after successful withdrawal
        await fetchCoins();
      } else {
        showMessage("Withdrawal failed", "error");
        await delayAction(1500);
      }
    } catch (error) {
      console.error("Withdrawal error:", error);
      showMessage("Withdrawal failed: " + (error as Error).message, "error");
      await delayAction(1500);
    } finally {
      setWithdrawing(false);
      try {
        if (closeMiniApp.isAvailable()) {
          closeMiniApp();
        }
      } catch (e) {
        console.log("Could not close mini app:", e);
        // Fallback: show a message to user that they can close manually
        showMessage(
          "Transaction completed! You can close this tab.",
          "success"
        );
      }
    }
  };

  const handleCoinSelect = (coin: ICoin) => {
    setSelectedCoin(coin);
    setAmount("");
  };

  const handleMaxAmount = () => {
    if (selectedCoin) {
      const maxAmount = formatCoinAmount(selectedCoin).replace(/,/g, "");
      setAmount(maxAmount);
    }
  };

  if (loading) {
    return (
      <Page>
        <Section>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              padding: "40px 20px",
            }}
          >
            <div
              style={{
                width: "40px",
                height: "40px",
                border: "3px solid var(--tg-theme-hint-color, #e0e0e0)",
                borderTop: "3px solid var(--tg-theme-button-color, #007AFF)",
                borderRadius: "50%",
                animation: "spin 1s linear infinite",
                marginBottom: "16px",
              }}
            ></div>
            <Text>Loading your coins...</Text>
          </div>
        </Section>
      </Page>
    );
  }

  return (
    <Page>
      <Message message={message} />

      <div style={{ padding: "10px 10px 20px 10px" }}>
        <Title style={{ padding: "20px 20px 10px 20px", margin: 0 }}>
          Withdraw Funds
        </Title>
        <Caption
          style={{
            padding: "0px 20px 24px 0",
            margin: 0,
            color: "var(--tg-theme-hint-color, #999999)",
            lineHeight: "1.4",
          }}
        >
          Select a token and amount to withdraw from your Quark account
        </Caption>
      </div>

      {coins.length === 0 ? (
        <Section>
          <div
            style={{
              textAlign: "center",
              padding: "40px 20px",
            }}
          >
            <div style={{ marginBottom: "16px", fontSize: "48px" }}>💰</div>
            <Text weight="2">No coins available</Text>
            <Caption style={{ marginTop: "8px" }}>
              You don&apos;t have any coins with a balance greater than 0
            </Caption>
          </div>
        </Section>
      ) : (
        <>
          {/* Coin Selection */}
          <Section header="Select Token">
            <List
              style={{
                backgroundColor: "var(--tg-theme-secondary-bg-color, #F8F9FA)",
                borderRadius: "16px",
                margin: "0 12px",
                overflow: "hidden",
                border:
                  "2px solid var(--tg-theme-section-separator-color, #E1E3E6)",
                padding: "8px",
              }}
            >
              {coins.map((coin, index) => (
                <Cell
                  key={`${coin.asset_type}-${index}`}
                  before={
                    <Avatar
                      size={40}
                      style={{
                        backgroundColor:
                          "var(--tg-theme-button-color, #007AFF)",
                        color: "var(--tg-theme-button-text-color, #FFFFFF)",
                      }}
                    >
                      <Text
                        style={{
                          fontSize: "18px",
                          fontWeight: "600",
                          color: "inherit",
                        }}
                      >
                        {getCoinSymbol(coin).charAt(0)}
                      </Text>
                    </Avatar>
                  }
                  after={
                    <div style={{ textAlign: "right" }}>
                      <Text weight="2">{formatCoinAmount(coin)}</Text>
                      <Caption>{getCoinSymbol(coin)}</Caption>
                    </div>
                  }
                  subtitle={
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "8px",
                        marginTop: "4px",
                      }}
                    >
                      <Badge
                        type="number"
                        mode={
                          getTokenStandard(coin) === "v1"
                            ? "primary"
                            : "secondary"
                        }
                      >
                        {getTokenStandard(coin).toUpperCase()}
                      </Badge>
                      <Caption
                        style={{
                          color: "var(--tg-theme-hint-color, #999999)",
                        }}
                      >
                        {coin.asset_type?.slice(0, 10)}...
                        {coin.asset_type?.slice(-8)}
                      </Caption>
                    </div>
                  }
                  onClick={() => handleCoinSelect(coin)}
                  style={{
                    backgroundColor:
                      selectedCoin?.asset_type === coin.asset_type
                        ? "rgba(0, 122, 255, 0.15)"
                        : "var(--tg-theme-bg-color, #FFFFFF)",
                    borderRadius: "12px",
                    margin: "4px 8px",
                    padding: "16px",
                    opacity: 1,
                    border:
                      selectedCoin?.asset_type === coin.asset_type
                        ? "2px solid var(--tg-theme-button-color, #007AFF)"
                        : "2px solid var(--tg-theme-section-separator-color, #E1E3E6)",
                    transition: "all 0.2s ease",
                    boxShadow:
                      selectedCoin?.asset_type === coin.asset_type
                        ? "0 4px 16px rgba(0, 122, 255, 0.25)"
                        : "0 2px 8px rgba(0,0,0,0.08)",
                  }}
                >
                  <Text weight="2">{getCoinName(coin)}</Text>
                </Cell>
              ))}
            </List>
          </Section>

          {/* Withdrawal Form */}
          {selectedCoin && (
            <Section header="Withdrawal Amount">
              <Card style={{ margin: "0 16px" }}>
                <div style={{ padding: "20px" }}>
                  <div style={{ marginBottom: "16px" }}>
                    <Text
                      weight="2"
                      style={{ display: "block", marginBottom: "8px" }}
                    >
                      Amount to withdraw
                    </Text>
                    <div style={{ position: "relative" }}>
                      <Input
                        type="number"
                        value={amount}
                        onChange={(e) => setAmount(e.target.value)}
                        placeholder="0.00"
                        style={{
                          width: "100%",
                          fontSize: "18px",
                          padding: "16px 20px",
                          paddingRight: "80px",
                          backgroundColor:
                            "var(--tg-theme-secondary-bg-color, #F5F5F5)",
                          color: "var(--tg-theme-text-color, #000000)",
                          border:
                            "1px solid var(--tg-theme-hint-color, #E0E0E0)",
                          borderRadius: "12px",
                        }}
                      />
                      <Button
                        size="s"
                        mode="plain"
                        onClick={handleMaxAmount}
                        style={{
                          position: "absolute",
                          right: "12px",
                          top: "50%",
                          transform: "translateY(-50%)",
                          padding: "8px 12px",
                          backgroundColor:
                            "var(--tg-theme-button-color, #007AFF)",
                          color: "var(--tg-theme-button-text-color, #FFFFFF)",
                          borderRadius: "8px",
                          fontWeight: "600",
                          fontSize: "12px",
                          minWidth: "48px",
                          border: "none",
                        }}
                      >
                        MAX
                      </Button>
                    </div>
                  </div>

                  <div
                    style={{
                      backgroundColor:
                        "var(--tg-theme-secondary-bg-color, #F5F5F5)",
                      borderRadius: "12px",
                      padding: "16px",
                      marginBottom: "20px",
                      border:
                        "1px solid var(--tg-theme-hint-color, rgba(0,0,0,0.1))",
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        marginBottom: "8px",
                      }}
                    >
                      <Caption>Available Balance:</Caption>
                      <Caption weight="2">
                        {formatCoinAmount(selectedCoin)}{" "}
                        {getCoinSymbol(selectedCoin)}
                      </Caption>
                    </div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        marginBottom: "8px",
                      }}
                    >
                      <Caption>Token Standard:</Caption>
                      <Badge
                        type="number"
                        mode={
                          getTokenStandard(selectedCoin) === "v1"
                            ? "primary"
                            : "secondary"
                        }
                      >
                        {getTokenStandard(selectedCoin).toUpperCase()}
                      </Badge>
                    </div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                      }}
                    >
                      <Caption>Token Address:</Caption>
                      <Caption
                        style={{
                          fontFamily: "monospace",
                          fontSize: "11px",
                          maxWidth: "150px",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          color: "var(--tg-theme-hint-color, #999999)",
                          backgroundColor: "var(--tg-theme-bg-color, #FFFFFF)",
                          padding: "2px 6px",
                          borderRadius: "4px",
                          border:
                            "1px solid var(--tg-theme-hint-color, rgba(0,0,0,0.1))",
                        }}
                      >
                        {selectedCoin.asset_type}
                      </Caption>
                    </div>
                  </div>

                  <Button
                    size="l"
                    stretched
                    onClick={handleWithdraw}
                    disabled={
                      !amount ||
                      parseFloat(amount) <= 0 ||
                      withdrawing ||
                      isDelaying
                    }
                    style={{
                      fontSize: "16px",
                      fontWeight: "600",
                      backgroundColor: "var(--tg-theme-button-color, #007AFF)",
                      color: "var(--tg-theme-button-text-color, #FFFFFF)",
                      border: "none",
                      borderRadius: "12px",
                      padding: "16px 24px",
                      boxShadow: "0 2px 8px rgba(0, 122, 255, 0.2)",
                      opacity: isDelaying ? 0.7 : 1,
                    }}
                  >
                    {withdrawing
                      ? "Processing..."
                      : isDelaying
                      ? "Please wait..."
                      : `Withdraw ${getCoinSymbol(selectedCoin)}`}
                  </Button>
                </div>
              </Card>
            </Section>
          )}
        </>
      )}

      <style jsx>{`
        @keyframes spin {
          0% {
            transform: rotate(0deg);
          }
          100% {
            transform: rotate(360deg);
          }
        }
      `}</style>
    </Page>
  );
}
