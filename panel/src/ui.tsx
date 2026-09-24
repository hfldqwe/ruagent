// Shared UI bridge: our small primitives on top of antd. Views keep calling
// useToast/Modal/Empty/Spinner — the implementations are antd's now.

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
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

// ---------------------------------------------------------------------------
// S2: draggable sider splitter (contract: docs/design/views/README.md:406)
// ---------------------------------------------------------------------------
// Shared mechanism only. Which column renders the handle, and what it does with
// the returned width, belongs to App.tsx / Chat.tsx. The rules live here so the
// wiring cannot get them wrong:
//   * never dragged  => width stays null => the owner keeps its own default and
//     writes NO inline width (228 / 248 stay bit-identical to today);
//   * min = the default itself (the contract derives it: the nav footer already
//     wraps at 228, and the chat rail's label collapses below 248);
//   * max = min(2 x default, viewport - other column - 390). 390 is the
//     measured narrowest usable content width; 2x is a DESIGN RULING (marked as
//     such in the contract, not a measurement);
//   * draggable only at viewport >= 1024: below that the chat rail is a drawer,
//     so dragging would fight the overflow-0 contract;
//   * APG window splitter: focusable, role=separator, aria-orientation=vertical,
//     aria-valuenow/min/max, arrows step 8px, Home/End jump to the bounds.

const RESIZE_STEP = 8;
const RESIZE_CONTENT_MIN = 390;
const RESIZE_GATE = 1024;

const resizeKey = (id: string) => "ruagent.sidebar." + id;

function readStoredWidth(id: string): number | null {
  try {
    const raw = window.localStorage.getItem(resizeKey(id));
    if (raw === null) return null;
    const n = Number(raw);
    return Number.isFinite(n) ? n : null;
  } catch {
    return null;
  }
}

/** Viewport width, kept in state so the drag gate reacts to a window resize. */
export function useViewportWidth(): number {
  const [w, setW] = useState(() =>
    typeof window === "undefined" ? RESIZE_GATE : window.innerWidth,
  );
  useEffect(() => {
    const onResize = () => setW(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return w;
}

export interface SidebarResizeOptions {
  /** Storage suffix: the width persists under "ruagent.sidebar.<id>". */
  id: string;
  /** The default width. It is also the minimum - see the contract derivation. */
  def: number;
  /** The OTHER column's current width, for the max formula. */
  other: number;
  contentMin?: number;
  /** Owner-side gate (e.g. "the sider is collapsed, do not drag"). */
  enabled?: boolean;
  /** Set for a column whose handle sits on its LEFT edge. */
  invert?: boolean;
}

export interface SidebarResize {
  /** null until the user drags: the owner must then render its own default. */
  width: number | null;
  /** width ?? def, clamped - what aria-valuenow reports. */
  value: number;
  min: number;
  max: number;
  canDrag: boolean;
  dragging: boolean;
  setWidth: (w: number) => void;
  reset: () => void;
  /** Spread onto <ResizeHandle {...handleProps} label={...} />. */
  handleProps: {
    value: number;
    min: number;
    max: number;
    disabled: boolean;
    invert: boolean;
    dragging: boolean;
    onChange: (w: number) => void;
    onDragStateChange: (dragging: boolean) => void;
  };
}

export function useSidebarResize({
  id,
  def,
  other,
  contentMin = RESIZE_CONTENT_MIN,
  enabled = true,
  invert = false,
}: SidebarResizeOptions): SidebarResize {
  const viewport = useViewportWidth();
  const [stored, setStored] = useState<number | null>(() => readStoredWidth(id));
  const [dragging, setDragging] = useState(false);
  const min = def;
  const max = Math.max(min, Math.min(2 * def, viewport - other - contentMin));
  const clamp = useCallback((w: number) => Math.min(max, Math.max(min, Math.round(w))), [max, min]);
  const canDrag = enabled && viewport >= RESIZE_GATE;
  const width = stored === null ? null : clamp(stored);
  const value = width === null ? min : width;
  const setWidth = useCallback(
    (w: number) => {
      const next = clamp(w);
      setStored(next);
      try {
        window.localStorage.setItem(resizeKey(id), String(next));
      } catch {
        /* Storage is a nicety here, never a gate: a private-mode failure must
           not break dragging. */
      }
    },
    [clamp, id],
  );
  const reset = useCallback(() => {
    setStored(null);
    try {
      window.localStorage.removeItem(resizeKey(id));
    } catch {
      /* see setWidth */
    }
  }, [id]);
  return {
    width,
    value,
    min,
    max,
    canDrag,
    dragging,
    setWidth,
    reset,
    handleProps: {
      value,
      min,
      max,
      disabled: !canDrag,
      invert,
      dragging,
      onChange: setWidth,
      onDragStateChange: setDragging,
    },
  };
}

export interface ResizeHandleProps {
  value: number;
  min: number;
  max: number;
  /** Required: the wiring owns i18n, this file does not add dictionary keys. */
  label: string;
  disabled?: boolean;
  invert?: boolean;
  dragging?: boolean;
  onChange: (w: number) => void;
  onDragStateChange?: (dragging: boolean) => void;
}

/**
 * APG window splitter, rendered as the contract's 1px rule line (rules-not-boxes:
 * the element stays <= 2px wide in every state, including hover).
 */
export function ResizeHandle({
  value,
  min,
  max,
  label,
  disabled = false,
  invert = false,
  dragging = false,
  onChange,
  onDragStateChange,
}: ResizeHandleProps) {
  const origin = useRef<{ x: number; w: number } | null>(null);
  const dir = invert ? -1 : 1;

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (disabled) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      onChange(value - dir * RESIZE_STEP);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      onChange(value + dir * RESIZE_STEP);
    } else if (e.key === "Home") {
      e.preventDefault();
      onChange(min);
    } else if (e.key === "End") {
      e.preventDefault();
      onChange(max);
    }
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (disabled || e.button !== 0) return;
    e.preventDefault();
    origin.current = { x: e.clientX, w: value };
    e.currentTarget.setPointerCapture?.(e.pointerId);
    onDragStateChange?.(true);
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const from = origin.current;
    if (!from) return;
    onChange(from.w + dir * (e.clientX - from.x));
  };
  const endDrag = () => {
    if (!origin.current) return;
    origin.current = null;
    onDragStateChange?.(false);
  };

  return (
    <div
      className={"resize-handle" + (dragging ? " dragging" : "")}
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-disabled={disabled || undefined}
      tabIndex={disabled ? -1 : 0}
      onKeyDown={onKeyDown}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onLostPointerCapture={endDrag}
    />
  );
}

