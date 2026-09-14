// Shared UI bridge: our small primitives on top of antd. Views keep calling
// useToast/Modal/Empty/Spinner — the implementations are antd's now.

import { useEffect, type ReactNode } from "react";
import { App as AntApp, Empty as AntEmpty, Modal as AntModal, Spin } from "antd";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { IconName } from "./icons";
import { Icon } from "./icons";
import { relTime, useI18n } from "./i18n";

// ---------------------------------------------------------------------------
// Toasts -> antd message (via App context)
// ---------------------------------------------------------------------------

type MessageApi = ReturnType<typeof AntApp.useApp>["message"];

let messageApi: MessageApi | null = null;

/** Mounts once inside App; captures antd's message instance. */
export function ToastBridge() {
  const { message } = AntApp.useApp();
  useEffect(() => {
    messageApi = message;
  }, [message]);
  return null;
}

export const useToast = () =>
  (kind: "ok" | "err", text: string) => {
    if (messageApi) messageApi.open({ type: kind === "ok" ? "success" : "error", content: text });
  };

// Kept for App.tsx compatibility — the provider is antd's now.
export const ToastHost = ({ children }: { children: ReactNode }) => <>{children}</>;

// ---------------------------------------------------------------------------
// Modal -> antd Modal
// ---------------------------------------------------------------------------

export function Modal({
  title,
  onClose,
  children,
  wide,
  footer,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
  footer?: ReactNode;
}) {
  return (
    <AntModal
      title={title}
      open
      onCancel={onClose}
      width={wide ? 760 : 560}
      footer={footer ?? null}
      destroyOnHidden
    >
      {children}
    </AntModal>
  );
}

// ---------------------------------------------------------------------------
// States -> antd Spin / Empty
// ---------------------------------------------------------------------------

export const Spinner = ({ label }: { label?: string }) => (
  <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 12, padding: "72px 0" }}>
    <Spin size="large" />
    {label ? <span className="muted">{label}</span> : null}
  </div>
);

export function Empty({
  icon,
  title,
  hint,
  action,
}: {
  icon: IconName;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <AntEmpty
      image={<Icon name={icon} size={44} style={{ opacity: 0.35 }} />}
      imageStyle={{ height: 52, display: "flex", justifyContent: "center" }}
      description={
        <div>
          <div style={{ fontWeight: 600 }}>{title}</div>
          {hint ? (
            <div className="muted" style={{ maxWidth: 420, margin: "4px auto 0", fontSize: 13 }}>
              {hint}
            </div>
          ) : null}
        </div>
      }
      style={{ padding: "56px 0" }}
    >
      {action}
    </AntEmpty>
  );
}

// ---------------------------------------------------------------------------
// Relative time (locale-aware)
// ---------------------------------------------------------------------------

export function RelTime({ iso }: { iso: string | null | undefined }) {
  const { lang } = useI18n();
  return <>{relTime(iso, lang)}</>;
}

// ---------------------------------------------------------------------------
// Status — colors valid on both light and dark surfaces
// ---------------------------------------------------------------------------

// Theme-aware status colors: CSS vars defined per mode in index.css
// (--status-ok/err/warn/idle), amber for the active states. Dots are
// non-text marks (3:1 floor on every surface); pill text uses the same
// vars and clears 4.5:1 in both modes.
const STATUS_COLORS: Record<string, string> = {
  done: "var(--status-ok)",
  completed: "var(--status-ok)",
  failed: "var(--status-err)",
  cancelled: "var(--status-err)",
  interrupted: "var(--status-warn)",
  in_progress: "var(--ant-color-primary)",
  running: "var(--ant-color-primary)",
  spawning: "var(--ant-color-primary)",
  queued: "var(--status-idle)",
  pending: "var(--status-idle)",
  waiting_permission: "var(--status-warn)",
  blocked: "var(--status-warn)",
};

export function StatusDot({ status }: { status: string }) {
  return (
    <span
      className="dot"
      style={{ background: STATUS_COLORS[status] ?? "var(--status-idle)" }}
    />
  );
}

export function StatusPill({ status }: { status: string }) {
  const { t } = useI18n();
  return (
    <span
      className="pill"
      style={{
        color: STATUS_COLORS[status] ?? "inherit",
        borderColor: STATUS_COLORS[status] ?? "var(--ant-color-border)",
      }}
    >
      {t(`status.${status}`)}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

export function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

const usd = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });
const usdPrecise = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 4,
});

export function fmtUsd(n: number | null | undefined): string {
  if (n == null) return "—";
  if (n === 0) return "$0";
  if (n < 0.01) return usdPrecise.format(n);
  return usd.format(n);
}

export function UsageMeter({ used, size }: { used: number; size: number }) {
  const pct = size > 0 ? Math.min(100, (used / size) * 100) : 0;
  return (
    <span title={`${fmtTokens(used)} / ${fmtTokens(size)}`} className="mono" style={{ fontSize: 12 }}>
      {fmtTokens(used)}/{fmtTokens(size)} · {pct.toFixed(0)}%
    </span>
  );
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

export function Markdown({ children }: { children: string }) {
  return (
    <div className="md">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{children}</ReactMarkdown>
    </div>
  );
}
