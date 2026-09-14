// Theme: antd ConfigProvider + dark/light mode (persisted) + the surface
// system from docs/design-study/multica-tokens.css translated onto antd's
// design tokens.
//
// Surface architecture (Multica): the app shell is the quiet outer frame;
// the page canvas is where lists, boards and conversations live; surfaces
// are bounded content groups; raised surfaces are ephemeral overlays. In
// dark mode higher = lighter; each step is ~0.03 lightness apart:
//   app-shell .155 < canvas .18 < surface .21 < raised .235 < hover .274 < selected .30
// Values are Multica's OKLCH tokens converted to sRGB hex; the handful of
// roles antd has no token for live as CSS vars in index.css — keep in sync.

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

// Inter leads where installed; Segoe UI Variable is the Windows-native
// equivalent. CJK falls through PingFang SC -> Microsoft YaHei so zh copy
// never lands on the browser's default face.
const FONT =
  '"Inter", "Segoe UI Variable Text", "Segoe UI", system-ui, -apple-system, "PingFang SC", "Microsoft YaHei", "Noto Sans SC", sans-serif';
const MONO = '"IBM Plex Mono", "Cascadia Code", ui-monospace, Consolas, monospace';

const D = {
  appShell: "#0c0c0e", // oklch(0.155 0.005 285.823)
  pageCanvas: "#111114", // oklch(0.18 0.005 285.823)
  surface: "#18181b", // oklch(0.21 0.006 285.885)
  surfaceRaised: "#1e1e21", // oklch(0.235 0.007 285.885)
  surfaceHover: "#27272a", // oklch(0.274 0.006 286.033)
  surfaceSelected: "#2d2d31", // oklch(0.3 0.006 286.033)
  border: "rgba(255, 255, 255, 0.10)", // oklch(1 0 0 / 10%)
  borderSecondary: "rgba(255, 255, 255, 0.06)", // oklch(1 0 0 / 6%)
  text: "#fafafa", // oklch(0.985 0 0)
  textSecondary: "#9f9fa9", // oklch(0.705 0.015 286.067) — 5.2:1+ on every surface
  textTertiary: "#8e8e98", // oklch(0.65 0.015 286.067)
  textQuaternary: "#7f7f89", // oklch(0.60 0.015 286.067) — faint marks, 3:1 floor
  brand: "#e79c27", // oklch(0.75 0.15 72) — signal amber; ink on it 8.1:1
  ink: "#17130c", // near-black text on the amber primary
  success: "#4aa651", // oklch(0.65 0.15 145)
  warning: "#cb9400", // oklch(0.70 0.16 85)
  error: "#ff6467", // oklch(0.704 0.191 22.216)
  menuShadow: "0 10px 28px rgba(0, 0, 0, 0.30), 0 2px 8px rgba(0, 0, 0, 0.18)",
  floatingShadow: "0 20px 48px rgba(0, 0, 0, 0.46), 0 4px 12px rgba(0, 0, 0, 0.28)",
  navSelectedBg: "#352b17", // amber 14% over surface — 6.1:1 for amber text
};

const L = {
  appShell: "#f3f3f4", // oklch(0.964435 0.001327 286.375)
  pageCanvas: "#fbfbfb", // oklch(0.988087 0 0)
  surface: "#ffffff", // oklch(1 0 0)
  surfaceRaised: "#ffffff", // oklch(1 0 0) — menus/modals read as raised via --menu-shadow
  surfaceHover: "#f4f4f5", // oklch(0.967 0.001 286.375)
  surfaceSelected: "#eeeef0", // oklch(0.95 0.002 286.375)
  border: "#e4e4e7", // oklch(0.92 0.004 286.32)
  borderSecondary: "#ececef", // oklch(0.945 0.003 286.32)
  text: "#09090b", // oklch(0.141 0.005 285.823)
  textSecondary: "#64636e", // oklch(0.505 0.016 285.938) — 4.88:1+ on every surface
  textTertiary: "#70707b", // oklch(0.55 0.016 285.938)
  textQuaternary: "#81818b", // oklch(0.606 0.016 285.938) — faint marks, 3:1 floor
  brand: "#9c620f", // oklch(0.547 0.115 68) — 5.0:1 as text and under white text
  success: "#1c882d", // oklch(0.55 0.16 145)
  warning: "#9d6400", // oklch(0.55 0.13 75) — darker than Multica's fill-only warning so it holds as text
  error: "#e7000b", // oklch(0.577 0.245 27.325)
  menuShadow: "0 8px 24px rgba(15, 23, 42, 0.08), 0 2px 6px rgba(15, 23, 42, 0.05)",
  floatingShadow: "0 16px 40px rgba(15, 23, 42, 0.14), 0 3px 10px rgba(15, 23, 42, 0.08)",
  navSelectedBg: "#f7f2ec", // amber 8% over white — 4.6:1 for amber text
};

// Type scale (Multica): micro 11 / caption 12 / label 13 / body 14 /
// body-lg 15 / title-sm 16. antd gets the three it has tokens for; the
// rest are spelled out per-rule in index.css with their locked line-heights.
// Radius ladder derives from one base: md 6 (rows/menus) / lg 8 (controls) /
// xl 12 (cards/dialogs).
const base = {
  fontFamily: FONT,
  fontFamilyCode: MONO,
  fontSize: 14,
  fontSizeSM: 12,
  fontSizeLG: 16,
  lineHeight: 1.5,
  lineHeightSM: 1.45,
  lineHeightLG: 1.5,
  borderRadius: 8,
  borderRadiusSM: 6,
  borderRadiusLG: 12,
  controlHeight: 32,
};

const darkTokens = {
  ...base,
  colorPrimary: D.brand,
  colorInfo: D.brand,
  colorLink: D.brand,
  // Primary buttons are amber — white on amber is 2.4:1, ink is 8:1.
  colorTextLightSolid: D.ink,
  colorBgBase: D.surface,
  colorBgContainer: D.surface,
  colorBgElevated: D.surfaceRaised,
  colorBgLayout: D.pageCanvas,
  colorBorder: D.border,
  colorBorderSecondary: D.borderSecondary,
  colorText: D.text,
  colorTextSecondary: D.textSecondary,
  colorTextTertiary: D.textTertiary,
  colorTextQuaternary: D.textQuaternary,
  colorSuccess: D.success,
  colorWarning: D.warning,
  colorError: D.error,
  boxShadow: D.menuShadow,
  boxShadowSecondary: D.floatingShadow,
};

const lightTokens = {
  ...base,
  colorPrimary: L.brand,
  colorInfo: L.brand,
  colorLink: L.brand,
  colorBgBase: L.pageCanvas,
  colorBgContainer: L.surface,
  colorBgElevated: L.surfaceRaised,
  colorBgLayout: L.pageCanvas,
  colorBorder: L.border,
  colorBorderSecondary: L.borderSecondary,
  colorText: L.text,
  colorTextSecondary: L.textSecondary,
  colorTextTertiary: L.textTertiary,
  colorTextQuaternary: L.textQuaternary,
  colorSuccess: L.success,
  colorWarning: L.warning,
  colorError: L.error,
  boxShadow: L.menuShadow,
  boxShadowSecondary: L.floatingShadow,
};

function components(mode: Mode) {
  const p = mode === "dark" ? D : L;
  return {
    Layout: {
      // The sidebar is its own panel one step off the canvas: raised in
      // dark (Multica --sidebar), the quiet frame grey in light.
      siderBg: mode === "dark" ? D.surface : L.appShell,
      headerBg: "transparent",
      bodyBg: p.pageCanvas,
    },
    Menu: {
      // Both variants are pinned because the dark algorithm flips Menu to
      // its dark* tokens; values are identical either way.
      itemBg: "transparent",
      darkItemBg: "transparent",
      itemHeight: 34,
      itemMarginInline: 8,
      itemBorderRadius: 6,
      groupTitleFontSize: 11,
      itemColor: p.textSecondary,
      darkItemColor: p.textSecondary,
      itemHoverBg: p.surfaceHover,
      darkItemHoverBg: p.surfaceHover,
      itemHoverColor: p.text,
      darkItemHoverColor: p.text,
      // Active nav is the one place the amber accent lives in the shell.
      itemSelectedBg: p.navSelectedBg,
      darkItemSelectedBg: p.navSelectedBg,
      itemSelectedColor: p.brand,
      darkItemSelectedColor: p.brand,
      groupTitleColor: p.textQuaternary,
      activeBarHeight: 0,
      activeBarBorderWidth: 0,
    },
    Card: { paddingLG: 18 },
    Table: {
      cellPaddingBlock: 10,
      headerBg: "transparent",
      headerColor: p.textSecondary,
      headerSplitColor: p.borderSecondary,
    },
    Modal: { titleFontSize: 16 },
    // Flat buttons — the material voice comes from surfaces, not glows.
    Button: { primaryShadow: "none", defaultShadow: "none", dangerShadow: "none" },
  };
}

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
    // antd's css-var scope lives inside the provider; the app shell itself
    // is painted here so overscroll never flashes the wrong mode.
    document.documentElement.style.background = mode === "dark" ? D.appShell : L.appShell;
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
          components: components(mode),
        }}
      >
        <AntApp>{children}</AntApp>
      </ConfigProvider>
    </ModeCtx.Provider>
  );
}

export { MONO };
