// Shared UI bridge: our small primitives on top of antd. Views keep calling
// useToast/Modal/Empty/Spinner — the implementations are antd's now.

import { useEffect, type ReactNode } from "react";
import { Alert, App as AntApp, Button, Empty as AntEmpty, Modal as AntModal, Spin } from "antd";
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
  <div className="spinner-block">
    <Spin size="large" />
    {label ? <span className="muted">{label}</span> : null}
  </div>
);

/**
 * Error: must never read as an empty (a failed load is not "no data yet").
 * antd Alert carries role="alert"; retry is offered whenever the caller can.
 */
export function ErrorState({
  title,
  hint,
  onRetry,
  retryLabel,
}: {
  title: string;
  hint?: string;
  onRetry?: () => void;
  retryLabel?: string;
}) {
  return (
    <div className="error-state">
      <Alert
        type="error"
        showIcon
        message={title}
        description={hint}
        action={
          onRetry ? (
            <Button size="small" onClick={onRetry}>
              {retryLabel ?? "Retry"}
            </Button>
          ) : undefined
        }
      />
    </div>
  );
}

/** Any request resolves to one of three states - never to a wrong one. */
export type LoadState<T> =
  | { kind: "loading" }
  | { kind: "error"; err: unknown }
  | { kind: "ready"; data: T };

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
      className="empty-state"
      image={
        <span className="empty-mark">
          <Icon name={icon} size={24} />
        </span>
      }
      description={
        <div className="empty-copy">
          <div className="empty-title">{title}</div>
          {hint ? <p className="empty-hint">{hint}</p> : null}
        </div>
      }
    >
      {action ? <div className="empty-action">{action}</div> : null}
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

// Theme-aware status colors (CSS vars per mode in index.css).
//
// Colour encodes the liveness CLASS, the label carries the exact status:
// now (signal) / terminal-ok / terminal-bad / not started.
// interrupted is a past-tense terminal state, so it sits with cancelled;
// waiting_permission and blocked are literally "waiting on you" -> signal.
const STATUS_COLORS: Record<string, string> = {
  done: "var(--status-ok)",
  completed: "var(--status-ok)",
  failed: "var(--status-err)",
  cancelled: "var(--status-err)",
  interrupted: "var(--status-err)",
  in_progress: "var(--signal)",
  running: "var(--signal)",
  spawning: "var(--signal)",
  queued: "var(--status-idle)",
  pending: "var(--status-idle)",
  waiting_permission: "var(--signal)",
  blocked: "var(--signal)",
};

// Pill text is 11px on the chip, so live states need the accent TEXT grade
// (the fill grade lands at 2.75:1 there in light mode).
const STATUS_TEXT_COLORS: Record<string, string> = {
  ...STATUS_COLORS,
  in_progress: "var(--signal-text)",
  running: "var(--signal-text)",
  spawning: "var(--signal-text)",
  waiting_permission: "var(--signal-text)",
  blocked: "var(--signal-text)",
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
      style={{ color: STATUS_TEXT_COLORS[status] ?? "var(--ant-color-text-secondary)" }}
    >
      {t(`status.${status}`)}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Zone - the third container role: a rule + a label, never a box
// ---------------------------------------------------------------------------

/**
 * A section head plus its content, with no surface of its own (container tree:
 * zone / panel / overlay). The DOM is identical to hand-writing
 * .zone/.zone-head/.zone-title/.zone-note, and className lets a view keep its
 * frozen class alongside it (e.g. "kanban-col zone").
 */
export function Zone({
  title,
  note,
  actions,
  className,
  children,
}: {
  title?: ReactNode;
  note?: ReactNode;
  actions?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  const head = title || note || actions;
  return (
    <section className={className ? "zone " + className : "zone"}>
      {head ? (
        <div className="zone-head">
          {title ? <div className="zone-title">{title}</div> : null}
          {note ? <span className="zone-note">{note}</span> : null}
          <span className="grow" />
          {actions}
        </div>
      ) : null}
      {children}
    </section>
  );
}

// ---------------------------------------------------------------------------
// IconButton - the one icon-button shape
// ---------------------------------------------------------------------------

/** A 24x24 minimum hit area, and a name is required of every caller so an
 *  unnamed icon button cannot ship. */
export function IconButton({
  label,
  icon,
  onClick,
  size = 16,
  disabled,
  className,
  title,
}: {
  /** Accessible name (aria-label). */
  label: string;
  icon: IconName;
  onClick?: () => void;
  size?: number;
  disabled?: boolean;
  className?: string;
  title?: string;
}) {
  return (
    <button
      type="button"
      className={className ? "icon-btn " + className : "icon-btn"}
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      title={title ?? label}
    >
      <Icon name={icon} size={size} />
    </button>
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
    <span title={`${fmtTokens(used)} / ${fmtTokens(size)}`} className="mono">
      {fmtTokens(used)}/{fmtTokens(size)} · {pct.toFixed(0)}%
    </span>
  );
}

// ---------------------------------------------------------------------------
// Instrument strip — the readout row a view opens with.
//
// Every desk shows its gauges: 2-4 numbers at display size on one plate,
// separated by rules instead of boxed into identical cards. It is the one
// place type is allowed to be big, which is what gives each view a centre
// of gravity it otherwise lacks.
// ---------------------------------------------------------------------------

export interface Readout {
  key: string;
  label: string;
  value: ReactNode;
  /** Makes the gauge a jump-link into the view that owns the number. */
  onOpen?: () => void;
  /** Wears the signal color: the number means "live" or "waiting on you". */
  signal?: boolean;
}

/**
 * The page-opening gauge row. `grid` turns the strip into the <=520 2-up grid
 * (`.readout-strip.grid`), where four gauges in one row would each be ~70px
 * wide (README §3.4 X4). At >520 it is a no-op, so views pass it unconditionally
 * and still get the flex geometry at desk widths - the layout is shared-layer,
 * views must not write this CSS themselves.
 */
export function ReadoutStrip({ items, grid }: { items: Readout[]; grid?: boolean }) {
  return (
    <div className={grid ? "panel readout-strip grid" : "panel readout-strip"}>
      {items.map((it) => {
        const value = <span className={it.signal ? "readout signal" : "readout"}>{it.value}</span>;
        return it.onOpen ? (
          <button key={it.key} className="readout-btn" onClick={it.onOpen}>
            {value}
            <span className="readout-label">{it.label}</span>
          </button>
        ) : (
          <div key={it.key} className="readout-cell">
            {value}
            <span className="readout-label">{it.label}</span>
          </div>
        );
      })}
    </div>
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
