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
const MONO = '"IBM Plex Mono", "Cascadia Code", ui-monospace, Consolas, monospace';

// ruagent identity: a mission-control instrument panel. The data face
// is mono (IBM Plex), surfaces are warm graphite/paper, one signal
// amber; the dark mode is the hero — deeper, layered, LED glows.
const base = {
  borderRadius: 8,
  fontFamily: FONT,
  fontFamilyCode: MONO,
  fontSize: 13.5,
  controlHeight: 32,
};

const darkTokens = {
  ...base,
  colorPrimary: "#e8a13c",
  colorInfo: "#e8a13c",
  colorLink: "#e8a13c",
  colorBgBase: "#0d0c10",
  colorBgContainer: "#151318",
  colorBgElevated: "#1d1a21",
  colorBgLayout: "#0d0c10",
  colorBorder: "#39343f",
  colorBorderSecondary: "#262231",
  colorText: "#eee9df",
  colorTextSecondary: "#b0a89a",
  colorTextTertiary: "#8b8477",
};

const lightTokens = {
  ...base,
  colorPrimary: "#9c620f",
  colorInfo: "#9c620f",
  colorLink: "#9c620f",
  colorBgBase: "#faf8f2",
  colorBgContainer: "#ffffff",
  colorBgElevated: "#ffffff",
  colorBgLayout: "#f3f0e8",
  colorBorder: "#ddd6c7",
  colorBorderSecondary: "#e8e2d5",
  colorText: "#241f17",
  colorTextSecondary: "#5b5346",
  colorTextTertiary: "#7a7264",
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
    document.documentElement.style.background = mode === "dark" ? "#0d0c10" : "#faf8f2";
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
              siderBg: mode === "dark" ? "#0a090d" : "#f5f2ea",
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
