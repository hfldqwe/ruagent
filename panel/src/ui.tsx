// Shared UI primitives: toasts, modal, empty/loading states, relative
// time, status dots, markdown, meters.

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

// ---------------------------------------------------------------------------
// Toasts
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
      <div
        className={wide ? "modal wide" : "modal"}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h3>{title}</h3>
          <button className="link" onClick={onClose}>
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
  icon: string;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="state empty">
      <div className="empty-icon">{icon}</div>
      <strong>{title}</strong>
      {hint ? <p className="muted">{hint}</p> : null}
      {action}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Relative time
// ---------------------------------------------------------------------------

export function relTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const then = new Date(iso.endsWith("Z") || iso.includes("+") ? iso : `${iso}Z`).getTime();
  if (Number.isNaN(then)) return iso;
  const diff = (Date.now() - then) / 1000;
  if (diff < 10) return "just now";
  if (diff < 60) return `${Math.floor(diff)}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 86400 * 7) return `${Math.floor(diff / 86400)}d ago`;
  return new Date(then).toLocaleDateString();
}

export function dateOf(iso: string): string {
  return (iso.split("T")[0] || "").replace(/Z$/, "");
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

const STATUS_COLORS: Record<string, string> = {
  done: "#3fb950",
  completed: "#3fb950",
  failed: "#f85149",
  cancelled: "#f85149",
  interrupted: "#d29922",
  in_progress: "#58a6ff",
  running: "#58a6ff",
  spawning: "#58a6ff",
  queued: "#8b949e",
  pending: "#8b949e",
  waiting_permission: "#d29922",
  blocked: "#d29922",
};

export function StatusDot({ status }: { status: string }) {
  return <span className="dot" style={{ background: STATUS_COLORS[status] ?? "#8b949e" }} />;
}

export function StatusPill({ status }: { status: string }) {
  return (
    <span
      className="pill"
      style={{ color: STATUS_COLORS[status] ?? "#8b949e", borderColor: STATUS_COLORS[status] ?? "#8b949e" }}
    >
      {status.replaceAll("_", " ")}
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
  style: "currency", currency: "USD", maximumFractionDigits: 4,
});

export function fmtUsd(n: number | null | undefined): string {
  if (n == null) return "—";
  if (n === 0) return "$0";
  if (n < 0.01) return usdPrecise.format(n);
  return usd.format(n);
}

export function UsageMeter({ used, size }: { used: number; size: number }) {
  const pct = size > 0 ? Math.min(100, (used / size) * 100) : 0;
  const color = pct > 85 ? "#f85149" : pct > 60 ? "#d29922" : "#3fb950";
  return (
    <span className="meter" title={`${fmtTokens(used)} / ${fmtTokens(size)} tokens in context`}>
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
