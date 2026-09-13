// Shared UI primitives: toasts, modal, empty/loading states, status dots,
// markdown, meters. Text comes from i18n at call sites.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Icon, type IconName } from "./icons";
import { relTime, useI18n } from "./i18n";

// ---------------------------------------------------------------------------
// Toasts (aria-live)
// ---------------------------------------------------------------------------

interface Toast {
  id: number;
  kind: "ok" | "err";
  text: string;
}

const ToastCtx = createContext<(kind: "ok" | "err", text: string) => void>(() => {});

export function ToastHost({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const push = useCallback((kind: "ok" | "err", text: string) => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, kind, text }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4200);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={t.kind === "ok" ? "toast ok" : "toast err"}>
            {t.text}
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}

export const useToast = () => useContext(ToastCtx);

// ---------------------------------------------------------------------------
// Modal
// ---------------------------------------------------------------------------

export function Modal({
  title,
  onClose,
  children,
  wide,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className={wide ? "modal wide" : "modal"} onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h3>{title}</h3>
          <button className="link" onClick={onClose} aria-label="close">
            ✕
          </button>
        </div>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// States
// ---------------------------------------------------------------------------

export const Spinner = ({ label }: { label?: string }) => (
  <div className="state">
    <div className="spin" />
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
    <div className="state empty">
      <div className="empty-icon">
        <Icon name={icon} size={30} />
      </div>
      <strong>{title}</strong>
      {hint ? <p className="muted">{hint}</p> : null}
      {action}
    </div>
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
// Status
// ---------------------------------------------------------------------------

const STATUS_COLORS: Record<string, string> = {
  done: "var(--ok)",
  completed: "var(--ok)",
  failed: "var(--danger)",
  cancelled: "var(--danger)",
  interrupted: "var(--warn)",
  in_progress: "var(--accent)",
  running: "var(--accent)",
  spawning: "var(--accent)",
  queued: "var(--text-faint)",
  pending: "var(--text-faint)",
  waiting_permission: "var(--warn)",
  blocked: "var(--warn)",
};

export function StatusDot({ status }: { status: string }) {
  return <span className="dot" style={{ background: STATUS_COLORS[status] ?? "var(--text-faint)" }} />;
}

export function StatusPill({ status }: { status: string }) {
  const { t } = useI18n();
  return (
    <span
      className="pill"
      style={{
        color: STATUS_COLORS[status] ?? "var(--text-dim)",
        borderColor: STATUS_COLORS[status] ?? "var(--line-strong)",
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
  const color = pct > 85 ? "var(--danger)" : pct > 60 ? "var(--warn)" : "var(--ok)";
  return (
    <span className="meter" title={`${fmtTokens(used)} / ${fmtTokens(size)}`}>
      <span className="meter-fill" style={{ width: `${pct}%`, background: color }} />
      <span className="meter-label">
        {fmtTokens(used)}/{fmtTokens(size)}
      </span>
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
