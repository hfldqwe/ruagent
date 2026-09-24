// Row 59's measuring instruments: CVD simulation and colour difference.
//
// Both are pinned to a named model because the NUMBER depends on the model, and
// a criterion whose number moves with an unstated convention is not a criterion.
// MASTER §12 row 59 fixes them: CVD = Machado 2009 at severity 1.0, distance =
// CIE76 in Lab under D65. t153 measured the same pair as 1.6 under CIE76 and
// 2.0 under CIEDE2000 -- so the convention is stated, not assumed.

/** sRGB channel (0..1) -> linear light. */
function srgbToLinear(c) {
  return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}
function linearToSrgb(c) {
  const v = c <= 0.0031308 ? c * 12.92 : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;
  return Math.max(0, Math.min(1, v));
}

/** "#rrggbb" | "rgb(...)" -> [r, g, b] in 0..255. Returns null when unparseable. */
export function parseColor(input) {
  const s = String(input ?? "").trim();
  const hex = s.match(/^#([0-9a-f]{6})$/i);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  const short = s.match(/^#([0-9a-f]{3})$/i);
  if (short) {
    return short[1].split("").map((h) => parseInt(h + h, 16));
  }
  const rgb = s.match(/^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/i);
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])];
  return null;
}

// Machado, Oliveira & Fernandes (2009), severity 1.0. Applied to LINEAR RGB.
export const MACHADO_1_0 = {
  protanopia: [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
  ],
  deuteranopia: [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881],
  ],
  tritanopia: [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900],
  ],
};

/** Simulate one CVD type on an sRGB triple. Returns an sRGB triple. */
export function simulateCvd(rgb255, type) {
  const m = MACHADO_1_0[type];
  if (!m) return rgb255.slice();
  const lin = rgb255.map((c) => srgbToLinear(c / 255));
  const out = m.map((row) => row[0] * lin[0] + row[1] * lin[1] + row[2] * lin[2]);
  return out.map((c) => Math.round(linearToSrgb(c) * 255));
}

/** sRGB triple -> CIE Lab under D65. */
export function rgbToLab(rgb255) {
  const [r, g, b] = rgb255.map((c) => srgbToLinear(c / 255));
  const x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
  const y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
  const z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
  const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  const fx = f(x), fy = f(y), fz = f(z);
  return [116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)];
}

/** CIE76 colour difference: the plain Euclidean distance in Lab. */
export function deltaE76(a, b) {
  const la = rgbToLab(a), lb = rgbToLab(b);
  return Math.sqrt((la[0] - lb[0]) ** 2 + (la[1] - lb[1]) ** 2 + (la[2] - lb[2]) ** 2);
}

/**
 * Row 59's verdict: every PAIR of category colours must stay >= floor apart
 * under every CVD type. 9 colours => 36 pairs, x3 types.
 *
 * The pair set is built by enumeration, so it cannot quietly shrink: a shorter
 * list of colours is reported as a missing-object-set, not as a smaller problem.
 */
export function cvdPairVerdict(colors, floor, expectedColors) {
  const list = (colors || []).map((c) => ({ name: c.name, rgb: parseColor(c.value) })).filter((c) => c.rgb);
  if (!list.length) return { measured: false, pass: null, why: "no category colour resolved" };
  if (expectedColors && list.length < expectedColors)
    return {
      measured: false,
      pass: null,
      why: "only " + list.length + " of " + expectedColors + " category colours resolved -- the pair set would be smaller than the contract's",
    };
  const types = Object.keys(MACHADO_1_0);
  const bad = [];
  let pairs = 0;
  let min = Infinity;
  let minAt = null;
  for (let i = 0; i < list.length; i++) {
    for (let j = i + 1; j < list.length; j++) {
      for (const t of types) {
        pairs++;
        const d = deltaE76(simulateCvd(list[i].rgb, t), simulateCvd(list[j].rgb, t));
        if (d < min) {
          min = d;
          minAt = list[i].name + " ↔ " + list[j].name + " (" + t + ")";
        }
        if (d < floor) bad.push({ a: list[i].name, b: list[j].name, type: t, d: Math.round(d * 100) / 100 });
      }
    }
  }
  return {
    measured: true,
    colors: list.length,
    pairs,
    min: Math.round(min * 100) / 100,
    minAt,
    bad,
    pass: bad.length === 0,
  };
}
