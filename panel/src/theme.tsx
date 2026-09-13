// Theme: antd ConfigProvider + dark/light mode (persisted) + ruagent's
// warm-graphite/amber identity on top of antd's design tokens.

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { App as AntApp, ConfigProvider, theme as antdTheme } from "antd";
import zhCN from "antd/locale/zh_CN";
import enUS from "antd/locale/en_US";
import { useI18n } from "./i18n";

type Mode = "dark" | "light";

const ModeCtx = createContext<{ mode: Mode; toggle: () => void }>({
  mode: "dark",
  toggle: () => {},
});
export const useThemeMode = () => useContext(ModeCtx);

const FONT =
  '"Segoe UI Variable Text", "Segoe UI", system-ui, -apple-system, sans-serif';
const MONO = '"Cascadia Code", "Cascadia Mono", ui-monospace, Consolas, monospace';

// ruagent identity, expressed through antd tokens.
const shared = {
  colorPrimary: "#d9861f",
  colorInfo: "#d9861f",
  colorLink: "#d9861f",
  borderRadius: 8,
  fontFamily: FONT,
  fontSize: 13.5,
  controlHeight: 32,
};

const darkTokens = {
  ...shared,
  colorBgBase: "#131116",
  colorBgContainer: "#1a181e",
  colorBgElevated: "#232029",
  colorBgLayout: "#131116",
  colorBorder: "#383342",
  colorBorderSecondary: "#262330",
  colorText: "#ede7db",
  colorTextSecondary: "#b3ab9d",
  colorTextTertiary: "#8a8275",
};

const lightTokens = {
  ...shared,
  colorBgBase: "#faf8f3",
  colorBgContainer: "#ffffff",
  colorBgElevated: "#ffffff",
  colorBgLayout: "#f4f1ea",
  colorBorder: "#ded8cb",
  colorBorderSecondary: "#e8e3d8",
  colorText: "#26221b",
  colorTextSecondary: "#5c564a",
  colorTextTertiary: "#8a8275",
};

export function ThemeProvider({ children }: { children: ReactNode }) {
  const { lang } = useI18n();
  const [mode, setMode] = useState<Mode>(() => {
    // ?mode=light|dark deep link wins once, then persists normally.
    const url = new URLSearchParams(window.location.search).get("mode");
    if (url === "light" || url === "dark") {
      try {
        localStorage.setItem("ruagent.mode", url);
      } catch {
        /* ignore */
      }
      return url;
    }
    try {
      return (localStorage.getItem("ruagent.mode") as Mode) || "dark";
    } catch {
      return "dark";
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem("ruagent.mode", mode);
    } catch {
      /* ignore */
    }
    document.documentElement.dataset.mode = mode;
    document.documentElement.style.colorScheme = mode;
    // antd's css-var scope lives inside the provider; the page canvas
    // itself is painted here so overscroll never flashes the wrong mode.
    document.documentElement.style.background = mode === "dark" ? "#131116" : "#faf8f3";
  }, [mode]);

  return (
    <ModeCtx.Provider
      value={{ mode, toggle: () => setMode((m) => (m === "dark" ? "light" : "dark")) }}
    >
      <ConfigProvider
        locale={lang === "zh" ? zhCN : enUS}
        theme={{
          cssVar: { key: "ruagent" },
          algorithm: mode === "dark" ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm,
          token: mode === "dark" ? darkTokens : lightTokens,
          components: {
            Layout: {
              siderBg: mode === "dark" ? "#0e0d10" : "#f7f4ee",
              headerBg: "transparent",
              bodyBg: "transparent",
            },
            Menu: {
              // darkItemBg / light equivalents flow from the algorithm;
              // tighten the item sizing for a denser tool feel
              itemHeight: 36,
              itemMarginInline: 8,
              itemBorderRadius: 7,
              groupTitleFontSize: 11,
            },
            Card: { paddingLG: 18 },
            Table: { cellPaddingBlock: 10 },
            Modal: { titleFontSize: 15 },
          },
        }}
      >
        <AntApp>{children}</AntApp>
      </ConfigProvider>
    </ModeCtx.Provider>
  );
}

export { MONO };
