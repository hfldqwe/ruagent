// Theme: antd ConfigProvider + dark/light mode (persisted).
//
// Design language (2026-09-20 restyle): quiet graphite neutrals with ONE
// restrained indigo accent — premium minimalism, no ornament. Surfaces
// separate by lightness alone (dark: higher = lighter, steps ~0.03 apart;
// no shadows on static cards — hairlines and elevation carry the depth).
// The accent appears only where it means something: primary buttons, the
// selected nav item, links, focus rings. Brand colors live per-runtime
// (brand.tsx), never as chrome.
//   app-shell < canvas < surface < raised < hover < selected

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

// Inter Variable is bundled (@fontsource-variable); Segoe UI Variable is
// the Windows-native fallback. CJK falls through PingFang SC -> Microsoft
// YaHei so zh copy never lands on the browser's default face.
const FONT =
  '"Inter Variable", "Segoe UI Variable Text", "Segoe UI", system-ui, -apple-system, "PingFang SC", "Microsoft YaHei", "Noto Sans SC", sans-serif';
const MONO = '"IBM Plex Mono", "Cascadia Code", ui-monospace, Consolas, monospace';

const D = {
  appShell: "#0a0a0c",
  pageCanvas: "#101013",
  surface: "#161619",
  surfaceRaised: "#1c1c20",
  surfaceHover: "#232329",
  surfaceSelected: "#2a2a33",
  border: "rgba(255, 255, 255, 0.085)",
  borderSecondary: "rgba(255, 255, 255, 0.05)",
  text: "#f5f5f7",
  textSecondary: "#a3a3ad", // 5.2:1+ on every surface
  textTertiary: "#8d8d98",
  textQuaternary: "#7e7e88", // 4.6:1+ — micro text carries real content (timestamps), audit 2026-09-20
  brand: "#5d6be0", // indigo — fills/borders (white on it 4.5:1)
  link: "#a7aeff", // brighter indigo for accent TEXT on dark (7:1+)
  ink: "#ffffff", // text on the indigo primary
  success: "#4aa651",
  warning: "#cb9400",
  error: "#ff6467",
  menuShadow: "0 10px 28px rgba(0, 0, 0, 0.30), 0 2px 8px rgba(0, 0, 0, 0.18)",
  floatingShadow: "0 20px 48px rgba(0, 0, 0, 0.46), 0 4px 12px rgba(0, 0, 0, 0.28)",
  navSelectedBg: "#222640", // indigo ~12% over surface
  navSelectedColor: "#aab2ff",
};

const L = {
  appShell: "#f3f3f4",
  pageCanvas: "#fbfbfb",
  surface: "#ffffff",
  surfaceRaised: "#ffffff", // menus/modals read as raised via --menu-shadow
  surfaceHover: "#f4f4f5",
  surfaceSelected: "#ededef",
  border: "#e4e4e7",
  borderSecondary: "#ececef",
  text: "#09090b",
  textSecondary: "#64636e", // 4.88:1+ on every surface
  textTertiary: "#70707b",
  textQuaternary: "#6b6b76", // 4.75:1+ on the app shell (audit 2026-09-20)
  brand: "#4f5ad6",
  link: "#4f5ad6", // 5.5:1+ as text on white
  ink: "#ffffff",
  success: "#1c882d",
  warning: "#9d6400",
  error: "#e7000b",
  menuShadow: "0 8px 24px rgba(15, 23, 42, 0.08), 0 2px 6px rgba(15, 23, 42, 0.05)",
  floatingShadow: "0 16px 40px rgba(15, 23, 42, 0.14), 0 3px 10px rgba(15, 23, 42, 0.08)",
  navSelectedBg: "#eef0fc", // indigo ~6% over white
  navSelectedColor: "#3f49c4",
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
  colorLink: D.link, // text-grade indigo — the seed is a fill color
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
  colorLink: L.link,
  colorTextLightSolid: L.ink,
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
      // dark, the quiet frame grey in light.
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
      // Active nav: the one accent in the shell — a quiet indigo wash and
      // a bright indigo label.
      itemSelectedBg: p.navSelectedBg,
      darkItemSelectedBg: p.navSelectedBg,
      itemSelectedColor: p.navSelectedColor,
      darkItemSelectedColor: p.navSelectedColor,
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
    Button: { primaryShadow: "none", defaultShadow: "none", dangerShadow: "none", fontWeight: 500 },
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
