// Theme: antd ConfigProvider + dark/light mode (persisted).
//
// Design language ("Bench", 2026-09-21 restyle): a control desk for a
// resident agent. Dark-first, two real materials per mode plus one signal.
//   - MATERIAL: shell < canvas < panel < raised. The steps are wide enough
//     to read as surfaces on a large dark field (canvas->panel is ~2.5x in
//     relative luminance); a user must never have to hunt for a border to
//     find the edge of a thing.
//   - SIGNAL: exactly one accent, amber. It means "now" — live, running,
//     waiting on you, and the primary action. Nothing else is colored.
//   - DOMAIN: entity/agent hues stay inside their views; they never become
//     chrome.
//   - TYPE: IBM Plex Sans for UI, IBM Plex Mono for real data. Hierarchy
//     comes from a scale that jumps (readouts are large), not from weight.
//
// Keep the hexes in sync with index.css.

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

// IBM Plex Sans Variable is bundled; it pairs with the bundled IBM Plex
// Mono so the whole panel speaks one family. Segoe UI Variable is the
// Windows-native fallback. CJK falls through PingFang SC -> Microsoft
// YaHei so zh copy never lands on the browser's default face.
const FONT =
  '"IBM Plex Sans Variable", "Segoe UI Variable Text", "Segoe UI", system-ui, -apple-system, "PingFang SC", "Microsoft YaHei", "Noto Sans SC", sans-serif';
const MONO = '"IBM Plex Mono", "Cascadia Code", ui-monospace, Consolas, monospace';

const D = {
  appShell: "#08090b",
  pageCanvas: "#0f1014",
  surface: "#1b1e24",
  surfaceRaised: "#262a31",
  surfaceHover: "#262a31",
  surfaceSelected: "#333844",
  border: "rgba(255, 255, 255, 0.13)",
  borderSecondary: "rgba(255, 255, 255, 0.07)",
  panelTop: "rgba(255, 255, 255, 0.055)",
  text: "#f4f5f7",
  textSecondary: "#adb2bb", // 7.8:1 on panel
  textTertiary: "#8e939c", // 5.4:1 on panel
  textQuaternary: "#838891", // 4.7:1 on panel / 5.4:1 on shell
  brand: "#f0a93b", // amber — fills (dark ink on it is 9.2:1)
  link: "#f5b457", // amber text grade (10:1 on canvas)
  ink: "#1a1206", // text on amber
  success: "#4fb477",
  warning: "#e2894f", // degraded/caution, not the signal amber
  error: "#f0615c",
  menuShadow: "0 12px 32px rgba(0, 0, 0, 0.44), 0 2px 8px rgba(0, 0, 0, 0.26)",
  floatingShadow: "0 24px 56px rgba(0, 0, 0, 0.56), 0 4px 14px rgba(0, 0, 0, 0.34)",
  navSelectedBg: "#2c2517", // amber wash
  navSelectedColor: "#f5b457",
};

const L = {
  appShell: "#eaebee",
  pageCanvas: "#f6f7f9",
  surface: "#ffffff",
  surfaceRaised: "#ffffff",
  surfaceHover: "#f0f1f4",
  surfaceSelected: "#e4e7ec",
  border: "#d6d9df",
  // t28 §12.7.1 (threshold row 30): the panel ring is 0 0 0 1px drawn *outside*
  // the panel box, so its backdrop is pageCanvas, not white. #e7e9ed measured
  // 1.1340:1 on the canvas (1.2155:1 on white); 1.2 is the system's hairline
  // floor, so the token moves and the target stays. #e7e9ed x 0.9615 keeps the
  // channel ratio 231:233:237 -> 222:224:228 (no new hue):
  // L = 0.744423 -> 1.2330:1 on canvas / 1.3217:1 on white.
  borderSecondary: "#dee0e4",
  panelTop: "rgba(255, 255, 255, 0)",
  text: "#0c0d10",
  textSecondary: "#5b6068", // 6.2:1 on white
  textTertiary: "#5f646d", // 5.95:1 on white / 4.99:1 on shell
  textQuaternary: "#656a72", // 5.44:1 on white / 4.57:1 on shell
  brand: "#c88413", // fill grade: 3.10:1 on white (dot/spine floor)
  link: "#8a5600", // 6.2:1 as text on white
  ink: "#241703",
  success: "#17793b",
  warning: "#8a4a12",
  error: "#c4121c",
  menuShadow: "0 10px 28px rgba(15, 20, 30, 0.10), 0 2px 6px rgba(15, 20, 30, 0.06)",
  floatingShadow: "0 20px 48px rgba(15, 20, 30, 0.16), 0 3px 10px rgba(15, 20, 30, 0.08)",
  navSelectedBg: "#fbf0da",
  navSelectedColor: "#7a4c00",
};

// Type scale: readouts 32/40 (mono), display 24/30, title 20/26, section
// 15/22, body 14/21, label 13/18, micro 11/16. antd gets the four it has
// tokens for; the rest are spelled out per-rule in index.css.
// Radius ladder: 4 chips, 6 controls, 10 panels/dialogs, 14 overlays.
const base = {
  fontFamily: FONT,
  fontFamilyCode: MONO,
  fontSize: 14,
  fontSizeSM: 12,
  fontSizeLG: 16,
  fontSizeXL: 20,
  fontSizeHeading1: 32,
  fontSizeHeading2: 24,
  fontSizeHeading3: 20,
  fontSizeHeading4: 16,
  fontSizeHeading5: 14,
  lineHeight: 1.5,
  lineHeightSM: 1.5,
  lineHeightLG: 1.4,
  lineHeightHeading1: 1.2,
  lineHeightHeading2: 1.25,
  lineHeightHeading3: 1.3,
  borderRadius: 6,
  borderRadiusSM: 4,
  borderRadiusLG: 10,
  controlHeight: 32,
  // One control height for the whole system (t36). antd derives its small
  // variant from controlHeightSM (default 24): Button's size=small block is
  // literally "controlHeight: token.controlHeightSM" and Input's small height
  // reads the same alias, so size=small rendered 24px tall next to 32px default
  // controls in the same view (and 24 next to 32 inside the same strip). The
  // small surface is bounded - measured across the 13 routes: 55 x .ant-btn-sm,
  // 1 x .ant-input-affix-wrapper-sm + 1 x .ant-input-sm, 3 x .ant-select-sm, no
  // .ant-table-sm / .ant-pagination-sm / .ant-segmented-sm at all - so one alias
  // unifies it and leaves no mixed tier behind. No !important, no per-rule CSS.
  controlHeightSM: 32,
  // antd's control-padding alias is 12 / 8, and every control subtracts
  // lineWidth (1) from it, so Select / Input / Segmented render 11px / 7px
  // horizontal padding - two values the spacing ladder does not have. 13 / 9
  // render as 12 / 8, i.e. on the ladder (threshold row 37). Control heights
  // are unaffected: they come from controlHeight, not from padding.
  controlPaddingHorizontal: 13,
  controlPaddingHorizontalSM: 9,
};

const darkTokens = {
  ...base,
  colorPrimary: D.brand,
  // Hover climbs the material ladder instead of darkening the accent.
  colorPrimaryHover: "#f5b457",
  colorPrimaryActive: "#f7c073",
  colorInfo: D.brand,
  colorLink: D.link,
  colorTextLightSolid: D.ink,
  colorBgBase: D.pageCanvas,
  colorBgContainer: D.surface,
  colorBgElevated: D.surfaceRaised,
  colorBgLayout: D.pageCanvas,
  colorFillQuaternary: "rgba(255, 255, 255, 0.05)",
  colorFillTertiary: "rgba(255, 255, 255, 0.08)",
  colorFillSecondary: "rgba(255, 255, 255, 0.12)",
  // Control edges are their own grade (3:1); rules stay on borderSecondary.
  colorBorder: "rgba(255, 255, 255, 0.34)",
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
  colorPrimaryHover: "#b8760f",
  colorPrimaryActive: "#a96a0c",
  colorInfo: L.brand,
  colorLink: L.link,
  colorTextLightSolid: L.ink,
  colorBgBase: L.pageCanvas,
  colorBgContainer: L.surface,
  colorBgElevated: L.surfaceRaised,
  colorBgLayout: L.pageCanvas,
  colorFillQuaternary: "rgba(15, 20, 30, 0.035)",
  colorFillTertiary: "rgba(15, 20, 30, 0.06)",
  colorFillSecondary: "rgba(15, 20, 30, 0.09)",
  colorBorder: "#8a9198", // 3.19:1 on white
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
      // The sidebar is the same panel material as everything else on the
      // canvas; the canvas is one step below it. Consistent in both modes.
      siderBg: p.surface,
      headerBg: "transparent",
      bodyBg: p.pageCanvas,
    },
    Menu: {
      // Both variants are pinned because the dark algorithm flips Menu to
      // its dark* tokens; values are identical either way.
      itemBg: "transparent",
      darkItemBg: "transparent",
      itemHeight: 36,
      itemMarginInline: 10,
      itemBorderRadius: 6,
      groupTitleFontSize: 11,
      itemColor: p.textSecondary,
      darkItemColor: p.textSecondary,
      itemHoverBg: p.surfaceHover,
      darkItemHoverBg: p.surfaceHover,
      itemHoverColor: p.text,
      darkItemHoverColor: p.text,
      // Active nav: the signal — amber label on an amber wash, plus the
      // rule-side spine drawn in index.css.
      itemSelectedBg: p.navSelectedBg,
      darkItemSelectedBg: p.navSelectedBg,
      itemSelectedColor: p.navSelectedColor,
      darkItemSelectedColor: p.navSelectedColor,
      groupTitleColor: p.textQuaternary,
      activeBarHeight: 0,
      activeBarBorderWidth: 0,
    },
    Card: { paddingLG: 20, headerFontSize: 15, headerHeight: 48 },
    // antd's own control padding is 15px (default) / 7px (small); the spacing
    // ladder has 16/8. Snapping it here converges ~200 off-ladder values per
    // route set (threshold row 37) and shifts each button 1px wider, nothing
    // else. paddingBlock is deliberately NOT pinned: 4.5px is antd's centring
    // remainder ((controlHeight - fontsize*lineHeight)/2 - lineWidth), not a
    // spacing choice, and pinning it to 4px was measured to shrink the input to
    // 31px inside a 32px control (it is what keeps the box 32px tall).
    Button: { primaryShadow: "none", defaultShadow: "none", dangerShadow: "none", fontWeight: 500,
      paddingInline: 16, paddingInlineSM: 8 },
    // The input's inline padding is derived from the global paddingSM alias
    // (12) minus lineWidth, i.e. 11 - off the ladder. Pinned here (component
    // scope) rather than by moving the alias, which would push every other
    // consumer of paddingSM to 13px. paddingBlock is left alone on purpose.
    Input: { paddingInline: 12 },
    Statistic: { contentFontSize: 32 },
    Table: {
      cellPaddingBlock: 10,
      headerBg: "transparent",
      headerColor: p.textSecondary,
      headerSplitColor: p.borderSecondary,
    },
    Modal: { titleFontSize: 18 },
    // Flat buttons — the material voice comes from surfaces, not glows.
    Segmented: { itemSelectedBg: p.surfaceRaised },
    // t28 §12.7.2 (threshold row 18): the switch's 42x21 track and 17px handle
    // are antd 6.6.3 *derived* values - trackHeight = fontSize x lineHeight
    // (14 x 1.5 = 21 here), i.e. a text line-height decided a control's height.
    // All three tokens must be given together: handleSize and trackMinWidth are
    // derived from trackHeight inside prepareComponentToken, and antd's internal
    // innerMinMargin/innerMaxMargin (handleSize / 2) would still use 17, so the
    // handle would travel to the wrong end. 24 / 20 / 48 = trackHeight,
    // trackHeight - 2 x trackPadding(2), 2 x trackHeight.
    Switch: { trackHeight: 24, handleSize: 20, trackMinWidth: 48 },
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
    // Browser chrome follows the shell. index.html ships TWO media-scoped
    // theme-color metas (dark #08090b / light #eaebee) whose *static* values
    // only serve the pre-JS first frame: there the browser picks one by OS
    // preference so the very first paint is not the wrong shade. Once JS runs,
    // the app's own theme is the fact - so EVERY theme-color meta is rewritten
    // to the current shell colour. Rewriting only the first one (the previous
    // behaviour) left the OS-light + app-dark case wrong: the meta the browser
    // actually reads is the one whose media query matches, i.e. the *second*
    // (light) one, which stayed at its static value - light chrome around a
    // dark app. Deliberately NOT "rewrite the meta that matches the OS
    // preference": that keeps the OS in charge of chrome and lets chrome drift
    // away from the app theme again on every preference change.
    const shell = mode === "dark" ? D.appShell : L.appShell;
    if (!document.querySelector('meta[name="theme-color"]')) {
      const fallback = document.createElement("meta");
      fallback.setAttribute("name", "theme-color");
      document.head.appendChild(fallback);
    }
    document
      .querySelectorAll('meta[name="theme-color"]')
      .forEach((m) => m.setAttribute("content", shell));
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
