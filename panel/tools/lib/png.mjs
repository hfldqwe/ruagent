// Minimal PNG decoder for the subset Chromium produces.
//
// Playwright screenshots are 8-bit, non-interlaced, colour type 2 (RGB) or
// 6 (RGBA) — no palette, no 16-bit, no Adam7. That is the whole contract;
// anything else throws instead of guessing. Zero dependencies beyond
// node:zlib (standard library), per the task's dependency rule.

import { inflateSync } from "node:zlib";

const SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/** Read the IHDR without decoding pixels (cheap pre-flight). */
export function pngHeader(buf) {
  if (buf.length < 33 || !buf.subarray(0, 8).equals(SIGNATURE)) {
    throw new Error("not a PNG");
  }
  if (buf.subarray(12, 16).toString("latin1") !== "IHDR") {
    throw new Error("PNG: first chunk is not IHDR");
  }
  return {
    width: buf.readUInt32BE(16),
    height: buf.readUInt32BE(20),
    depth: buf[24],
    colorType: buf[25],
    interlace: buf[28],
  };
}

/**
 * Decode a PNG buffer into { width, height, channels, data } where data is
 * row-major, `channels` bytes per pixel (3 = RGB, 4 = RGBA).
 */
export function decodePng(buf) {
  const hdr = pngHeader(buf);
  if (hdr.depth !== 8) throw new Error(`PNG: unsupported bit depth ${hdr.depth}`);
  if (hdr.interlace !== 0) throw new Error("PNG: interlaced images unsupported");
  const channels = hdr.colorType === 2 ? 3 : hdr.colorType === 6 ? 4 : 0;
  if (!channels) throw new Error(`PNG: unsupported colour type ${hdr.colorType}`);

  // Concatenate every IDAT payload, then inflate once.
  const parts = [];
  let off = 8;
  while (off + 8 <= buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.subarray(off + 4, off + 8).toString("latin1");
    const body = buf.subarray(off + 8, off + 8 + len);
    if (type === "IDAT") parts.push(body);
    else if (type === "IEND") break;
    off += 12 + len;
  }
  if (!parts.length) throw new Error("PNG: no IDAT chunks");
  const raw = inflateSync(Buffer.concat(parts));

  const { width, height } = hdr;
  const stride = width * channels;
  const out = Buffer.allocUnsafe(stride * height);
  const bpp = channels; // bytes per pixel — filter distance
  let prev = Buffer.alloc(stride); // scanline 0 has an all-zero prior row
  let pos = 0;
  for (let y = 0; y < height; y++) {
    const filter = raw[pos++];
    const line = raw.subarray(pos, pos + stride);
    pos += stride;
    const cur = out.subarray(y * stride, (y + 1) * stride);
    switch (filter) {
      case 0:
        line.copy(cur);
        break;
      case 1:
        for (let i = 0; i < stride; i++) {
          cur[i] = (line[i] + (i >= bpp ? cur[i - bpp] : 0)) & 0xff;
        }
        break;
      case 2:
        for (let i = 0; i < stride; i++) cur[i] = (line[i] + prev[i]) & 0xff;
        break;
      case 3:
        for (let i = 0; i < stride; i++) {
          const left = i >= bpp ? cur[i - bpp] : 0;
          cur[i] = (line[i] + ((left + prev[i]) >> 1)) & 0xff;
        }
        break;
      case 4:
        for (let i = 0; i < stride; i++) {
          const a = i >= bpp ? cur[i - bpp] : 0;
          const b = prev[i];
          const c = i >= bpp ? prev[i - bpp] : 0;
          const p = a + b - c;
          const pa = Math.abs(p - a);
          const pb = Math.abs(p - b);
          const pc = Math.abs(p - c);
          const pred = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
          cur[i] = (line[i] + pred) & 0xff;
        }
        break;
      default:
        throw new Error(`PNG: unknown filter ${filter} on row ${y}`);
    }
    prev = cur;
  }
  return { width, height, channels, data: out };
}

/** sRGB channel byte -> linear 0..1 (WCAG 2.x relative luminance input). */
export function srgbLinear(byte) {
  const c = byte / 255;
  return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

/** Relative luminance (WCAG 2.x) of an [r,g,b] byte triple. */
export function relLuma(r, g, b) {
  return 0.2126 * srgbLinear(r) + 0.7152 * srgbLinear(g) + 0.0722 * srgbLinear(b);
}

/**
 * Pixel statistics over a decoded screenshot, using MASTER.md appendix A's
 * formulas verbatim:
 *   luma  = (0.2126R + 0.7152G + 0.0722B) / 255, rounded to 2 decimals, bucketed
 *   chroma: S = (max-min)/max > 0.15 && max-min > 12
 *   signal: HSV hue 20-50deg, sat > 0.30, max-min > 25, max > 110
 * `region` (optional) restricts the scan to a rectangle.
 */
// `opts.exclude` is a list of exact [r,g,b] fills to leave OUT of the row-1
// luma-band count. MASTER §12 row 1's target is about how turbid the *designed
// surfaces* are; the app canvas is a single flat token whose share of the frame
// is a constant (measured: #0f1014 = 83.41% of an empty #inbox, luma 0.063 —
// inside the band), so counting it turns the metric into a proxy for "how empty
// is this page" rather than "how muddy is this surface". Both numbers are still
// returned: the original over every pixel, and the designed-surface one.
// Matching is exact because the canvas is a flat fill and the PNG is lossless;
// antialiasing only ever touches edges, which are not canvas pixels.
export function pixelStats(img, region, opts) {
  const { width, height, channels, data } = img;
  const x0 = Math.max(0, region?.x ?? 0);
  const y0 = Math.max(0, region?.y ?? 0);
  const x1 = Math.min(width, region ? (region.x ?? 0) + region.width : width);
  const y1 = Math.min(height, region ? (region.y ?? 0) + region.height : height);

  const exclude = (opts?.exclude ?? []).map((c) => (Array.isArray(c) ? c : [c.r, c.g, c.b]));
  const isCanvas = (r, g, b) => exclude.some((e) => e[0] === r && e[1] === g && e[2] === b);

  let total = 0;
  let band = 0; // 0.05 <= luma < 0.10
  let bandDesigned = 0; // same band, canvas pixels removed
  let designed = 0; // pixels that are not canvas
  let chroma = 0; // "real colour" pixels
  let signal = 0; // amber signal pixels
  let sumLuma = 0;
  const lumaHist = new Map(); // 0.00..1.00 -> count

  for (let y = y0; y < y1; y++) {
    let idx = (y * width + x0) * channels;
    for (let x = x0; x < x1; x++, idx += channels) {
      const r = data[idx];
      const g = data[idx + 1];
      const b = data[idx + 2];
      const canvas = exclude.length > 0 && isCanvas(r, g, b);
      if (!canvas) designed++;
      const lum = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
      const bucket = Math.round(lum * 100) / 100;
      lumaHist.set(bucket, (lumaHist.get(bucket) ?? 0) + 1);
      sumLuma += lum;
      if (bucket >= 0.05 && bucket < 0.1) {
        band++;
        if (!canvas) bandDesigned++;
      }

      const max = Math.max(r, g, b);
      const min = Math.min(r, g, b);
      const delta = max - min;
      if (max > 0 && delta / max > 0.15 && delta > 12) chroma++;
      if (delta > 25 && max > 110) {
        const sat = delta / max;
        if (sat > 0.3) {
          let hue;
          if (max === r) hue = 60 * (((g - b) / delta) % 6);
          else if (max === g) hue = 60 * ((b - r) / delta + 2);
          else hue = 60 * ((r - g) / delta + 4);
          if (hue < 0) hue += 360;
          if (hue >= 20 && hue <= 50) signal++;
        }
      }
      total++;
    }
  }
  const pct = (n) => (total ? (100 * n) / total : 0);
  const pctD = (n) => (designed ? (100 * n) / designed : 0);
  return {
    width: x1 - x0,
    height: y1 - y0,
    pixels: total,
    meanLuma: total ? sumLuma / total : 0,
    // Both readings, always. The original is never hidden — the row just judges
    // the one the captain's ruling names.
    lumaBandRatio: pct(band) / 100,
    lumaBandRatioDesigned: pctD(bandDesigned) / 100,
    designedPixels: designed,
    excludedPixels: total - designed,
    excludedRatio: total ? (total - designed) / total : 0,
    exclude: exclude.map((c) => "#" + c.map((v) => Number(v).toString(16).padStart(2, "0")).join("")),
    chromaRatio: pct(chroma) / 100,
    signalRatio: pct(signal) / 100,
    lumaHist: [...lumaHist.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 6)
      .map(([bucket, count]) => ({ luma: bucket, ratio: pct(count) / 100 })),
  };
}

/** One pixel as [r,g,b] (used by the self-test and by spot checks). */
export function pixelAt(img, x, y) {
  const i = (y * img.width + x) * img.channels;
  return [img.data[i], img.data[i + 1], img.data[i + 2]];
}

// ── CSS colour helpers, shared by the pixel pass and the Node-side
// post-processing (focus rings, graph edges). Same algebra as the in-page
// probe: parse -> composite -> WCAG ratio. ──────────────────────────────────
export function parseCssColor(s) {
  if (!s) return null;
  const t = String(s).trim();
  if (t === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  const hx = t.match(/^#([0-9a-f]{3,8})$/i);
  if (hx) {
    let h = hx[1];
    if (h.length === 3 || h.length === 4) h = h.split("").map((c) => c + c).join("");
    return {
      r: parseInt(h.slice(0, 2), 16),
      g: parseInt(h.slice(2, 4), 16),
      b: parseInt(h.slice(4, 6), 16),
      a: h.length === 8 ? parseInt(h.slice(6, 8), 16) / 255 : 1,
    };
  }
  const m = t.match(/^rgba?\(([^)]+)\)$/);
  if (!m) return null;
  const p = m[1].split(/[,\s/]+/).filter(Boolean).map(Number);
  if (p.length < 3 || p.slice(0, 3).some((n) => !Number.isFinite(n))) return null;
  return { r: p[0], g: p[1], b: p[2], a: p.length > 3 && Number.isFinite(p[3]) ? p[3] : 1 };
}

/** fg over an opaque bg (the audit always composites down to an opaque base). */
export function composite(fg, bg) {
  return {
    r: fg.r * fg.a + bg.r * (1 - fg.a),
    g: fg.g * fg.a + bg.g * (1 - fg.a),
    b: fg.b * fg.a + bg.b * (1 - fg.a),
    a: fg.a + bg.a * (1 - fg.a),
  };
}

/** WCAG 2.x contrast ratio between two {r,g,b} colours. */
export function contrastRatio(a, b) {
  const l1 = relLuma(a.r, a.g, a.b);
  const l2 = relLuma(b.r, b.g, b.b);
  return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
}

/** Raw relative-luminance step (MASTER appendix A: staircases use L1/L2). */
export function lumaStep(a, b) {
  const l1 = relLuma(a.r, a.g, a.b);
  const l2 = relLuma(b.r, b.g, b.b);
  return l1 >= l2 ? l1 / l2 : l2 / l1;
}
