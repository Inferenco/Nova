import type { PropsWithChildren } from "react";
import type { Metadata } from "next";
import { getLocale } from "next-intl/server";

import { Root } from "@/components/Root/Root";
import { I18nProvider } from "@/core/i18n/provider";

import "@telegram-apps/telegram-ui/dist/styles.css";
import "normalize.css/normalize.css";
import "./_assets/globals.css";
import { WalletProvider } from "@/context/WalletProvider";
import { AbiProvider } from "@/context/AbiProvider";
import { ChainProvider } from "@/context/ChainProvider";

export const metadata: Metadata = {
  title: "Quark AI",
  description:
    "Quark AI is a decentralized AI agent for the Telegram ecosystem.",
};

export default async function RootLayout({ children }: PropsWithChildren) {
  const locale = await getLocale();

  return (
    <html lang={locale} suppressHydrationWarning>
      <body>
        <I18nProvider>
          <ChainProvider>
            <WalletProvider>
              <AbiProvider>
                <Root>{children}</Root>
              </AbiProvider>
            </WalletProvider>
          </ChainProvider>
        </I18nProvider>
      </body>
    </html>
  );
}
