/**
 * A QR code, made here.
 *
 * The wallet reads codes with the camera — that scanner belongs to iOS and
 * Android — but showing one is drawing, and drawing is this side's work. It is
 * written out rather than installed because the whole of it is below: the
 * encoding is a fixed, published thing that has not moved since 2006, and a
 * package that draws squares is a package that reaches a process holding a
 * seed. Nothing here is a matter of taste; every table is from ISO/IEC 18004.
 *
 * **Byte mode, correction level M, versions 1 to 20.** That is 666 characters,
 * which is what an invitation needs: an out-of-band invitation URL carries a
 * `did:peer:2` — two keys and the mediator's address — inside base64, and
 * comes to four or five hundred characters. A code that would need more than
 * version 20 is not a code somebody is going to point a phone at, so
 * `encodeQr` refuses rather than carrying twenty more tables for a case that
 * does not arise.
 */

/** A finished code: `size` × `size` modules, true where the module is dark. */
export type QrMatrix = {
  /** Modules per side, quiet zone excluded. */
  size: number;
  /** Row-major, `modules[row][column]`. */
  modules: boolean[][];
};

/** More than the largest version carried here can hold. */
export class QrTooLong extends Error {}

/**
 * Per version, at correction level M: the error correction codewords each block
 * carries, then the blocks themselves as `[count, data codewords]` groups.
 *
 * A version's data capacity is the sum of its groups, and its total is that plus
 * one lot of correction codewords per block.
 */
const VERSIONS: { ec: number; groups: [number, number][] }[] = [
  { ec: 10, groups: [[1, 16]] },
  { ec: 16, groups: [[1, 28]] },
  { ec: 26, groups: [[1, 44]] },
  { ec: 18, groups: [[2, 32]] },
  { ec: 24, groups: [[2, 43]] },
  { ec: 16, groups: [[4, 27]] },
  { ec: 18, groups: [[4, 31]] },
  { ec: 22, groups: [[2, 38], [2, 39]] },
  { ec: 22, groups: [[3, 36], [2, 37]] },
  { ec: 26, groups: [[4, 43], [1, 44]] },
  { ec: 30, groups: [[1, 50], [4, 51]] },
  { ec: 22, groups: [[6, 36], [2, 37]] },
  { ec: 22, groups: [[8, 37], [1, 38]] },
  { ec: 24, groups: [[4, 40], [5, 41]] },
  { ec: 24, groups: [[5, 41], [5, 42]] },
  { ec: 28, groups: [[7, 45], [3, 46]] },
  { ec: 28, groups: [[10, 46], [1, 47]] },
  { ec: 26, groups: [[9, 43], [4, 44]] },
  { ec: 26, groups: [[3, 44], [11, 45]] },
  { ec: 26, groups: [[3, 41], [13, 42]] },
];

/**
 * Where the alignment patterns sit, per version, as coordinates that are used
 * both as rows and as columns. Version 1 has none.
 */
const ALIGNMENT: number[][] = [
  [],
  [6, 18],
  [6, 22],
  [6, 26],
  [6, 30],
  [6, 34],
  [6, 22, 38],
  [6, 24, 42],
  [6, 26, 46],
  [6, 28, 50],
  [6, 30, 54],
  [6, 32, 58],
  [6, 34, 62],
  [6, 26, 46, 66],
  [6, 26, 48, 70],
  [6, 26, 50, 74],
  [6, 30, 54, 78],
  [6, 30, 56, 82],
  [6, 30, 58, 86],
  [6, 34, 62, 90],
];

/* ------------------------------------------------------------------ GF(256) */

// The field the correction codewords are computed in: arithmetic modulo the
// primitive polynomial x⁸ + x⁴ + x³ + x² + 1, which is the one the standard
// names. Built once, so multiplication is two lookups and an addition.
const EXP = new Uint8Array(512);
const LOG = new Uint8Array(256);

for (let i = 0, x = 1; i < 255; i += 1) {
  EXP[i] = x;
  LOG[x] = i;
  x <<= 1;
  if (x & 0x100) {
    x ^= 0x11d;
  }
}
for (let i = 255; i < 512; i += 1) {
  EXP[i] = EXP[i - 255];
}

function multiply(a: number, b: number): number {
  return a === 0 || b === 0 ? 0 : EXP[LOG[a] + LOG[b]];
}

/** The generator polynomial for `count` correction codewords, highest term first. */
function generator(count: number): number[] {
  let poly = [1];
  for (let i = 0; i < count; i += 1) {
    const next = new Array<number>(poly.length + 1).fill(0);
    for (let j = 0; j < poly.length; j += 1) {
      next[j] ^= poly[j];
      next[j + 1] ^= multiply(poly[j], EXP[i]);
    }
    poly = next;
  }
  return poly;
}

/** The correction codewords for one block, as the remainder of a division. */
function remainder(data: number[], count: number): number[] {
  const divisor = generator(count);
  const working = [...data, ...new Array<number>(count).fill(0)];

  for (let i = 0; i < data.length; i += 1) {
    const lead = working[i];
    if (lead === 0) {
      continue;
    }
    for (let j = 0; j < divisor.length; j += 1) {
      working[i + j] ^= multiply(divisor[j], lead);
    }
  }

  return working.slice(data.length);
}

/* --------------------------------------------------------------- BCH codes */

function bch(value: number, generatorBits: number, degree: number): number {
  let rest = value << degree;
  const width = (bits: number) => (bits === 0 ? 0 : 32 - Math.clz32(bits));
  while (width(rest) - width(generatorBits) >= 0) {
    rest ^= generatorBits << (width(rest) - width(generatorBits));
  }
  return (value << degree) | rest;
}

/** The fifteen bits that say which correction level and which mask, protected. */
function formatBits(mask: number): number {
  // Level M is `0b00`, so the five data bits are the mask and two zeros above it.
  return bch(mask, 0x537, 10) ^ 0x5412;
}

/** The eighteen bits that say which version. Only versions 7 and up carry them. */
function versionBits(version: number): number {
  return bch(version, 0x1f25, 12);
}

/* ----------------------------------------------------------------- masking */

const MASKS: ((row: number, column: number) => boolean)[] = [
  (r, c) => (r + c) % 2 === 0,
  (r) => r % 2 === 0,
  (_r, c) => c % 3 === 0,
  (r, c) => (r + c) % 3 === 0,
  (r, c) => (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0,
  (r, c) => ((r * c) % 2) + ((r * c) % 3) === 0,
  (r, c) => (((r * c) % 2) + ((r * c) % 3)) % 2 === 0,
  (r, c) => (((r + c) % 2) + ((r * c) % 3)) % 2 === 0,
];

/**
 * How badly a masked code reads, by the four rules the standard scores with.
 * The mask with the lowest score is the one that gets drawn.
 */
function penalty(modules: boolean[][]): number {
  const size = modules.length;
  let score = 0;

  // Runs of five or more of one colour, along every row and every column.
  for (let a = 0; a < size; a += 1) {
    let rowRun = 1;
    let columnRun = 1;
    for (let b = 1; b < size; b += 1) {
      rowRun = modules[a][b] === modules[a][b - 1] ? rowRun + 1 : 1;
      if (rowRun === 5) score += 3;
      else if (rowRun > 5) score += 1;

      columnRun = modules[b][a] === modules[b - 1][a] ? columnRun + 1 : 1;
      if (columnRun === 5) score += 3;
      else if (columnRun > 5) score += 1;
    }
  }

  // Blocks of two by two in one colour.
  for (let row = 0; row < size - 1; row += 1) {
    for (let column = 0; column < size - 1; column += 1) {
      const first = modules[row][column];
      if (
        first === modules[row][column + 1] &&
        first === modules[row + 1][column] &&
        first === modules[row + 1][column + 1]
      ) {
        score += 3;
      }
    }
  }

  // The finder pattern's own signature appearing where there is no finder,
  // which is what would make a scanner look in the wrong place.
  const finder = [true, false, true, true, true, false, true, false, false, false, false];
  const matches = (at: (offset: number) => boolean | undefined, reversed: boolean) =>
    finder.every((wanted, offset) => at(reversed ? finder.length - 1 - offset : offset) === wanted);
  for (let row = 0; row < size; row += 1) {
    for (let column = 0; column <= size - finder.length; column += 1) {
      const across = (offset: number) => modules[row][column + offset];
      const down = (offset: number) => modules[column + offset][row];
      if (matches(across, false) || matches(across, true)) score += 40;
      if (matches(down, false) || matches(down, true)) score += 40;
    }
  }

  // How far the code is from being half dark.
  let dark = 0;
  for (const row of modules) {
    for (const module of row) {
      if (module) dark += 1;
    }
  }
  score += Math.floor(Math.abs((dark * 100) / (size * size) - 50) / 5) * 10;

  return score;
}

/* ------------------------------------------------------------- the drawing */

type Grid = {
  size: number;
  modules: boolean[][];
  /** True where a function pattern sits, which the data has to step around. */
  fixed: boolean[][];
};

function blank(size: number): Grid {
  return {
    size,
    modules: Array.from({ length: size }, () => new Array<boolean>(size).fill(false)),
    fixed: Array.from({ length: size }, () => new Array<boolean>(size).fill(false)),
  };
}

function place(grid: Grid, row: number, column: number, dark: boolean): void {
  grid.modules[row][column] = dark;
  grid.fixed[row][column] = true;
}

function drawFinder(grid: Grid, row: number, column: number): void {
  // The seven by seven eye, and the blank separator around it. Written with the
  // separator included so the loop covers both, clipped at the edges.
  for (let r = -1; r <= 7; r += 1) {
    for (let c = -1; c <= 7; c += 1) {
      const y = row + r;
      const x = column + c;
      if (y < 0 || y >= grid.size || x < 0 || x >= grid.size) {
        continue;
      }
      const ring = Math.max(Math.abs(r - 3), Math.abs(c - 3));
      place(grid, y, x, ring !== 2 && ring <= 3);
    }
  }
}

function drawFunctionPatterns(grid: Grid, version: number): void {
  const size = grid.size;

  drawFinder(grid, 0, 0);
  drawFinder(grid, 0, size - 7);
  drawFinder(grid, size - 7, 0);

  // The timing patterns, which give a scanner the module pitch.
  for (let i = 8; i < size - 8; i += 1) {
    place(grid, 6, i, i % 2 === 0);
    place(grid, i, 6, i % 2 === 0);
  }

  // Alignment patterns, at every crossing of the version's coordinates except
  // the three that would land on a finder.
  const centres = ALIGNMENT[version - 1];
  for (const row of centres) {
    for (const column of centres) {
      const onFinder =
        (row === 6 && column === 6) ||
        (row === 6 && column === size - 7) ||
        (row === size - 7 && column === 6);
      if (onFinder) {
        continue;
      }
      for (let r = -2; r <= 2; r += 1) {
        for (let c = -2; c <= 2; c += 1) {
          place(grid, row + r, column + c, Math.max(Math.abs(r), Math.abs(c)) !== 1);
        }
      }
    }
  }

  // The format areas are claimed here and written once a mask has been chosen:
  // fifteen modules around the top-left finder, and the same fifteen split
  // between the other two.
  for (let i = 0; i < 9; i += 1) {
    if (i !== 6) {
      place(grid, 8, i, false);
      place(grid, i, 8, false);
    }
  }
  for (let i = 0; i < 8; i += 1) {
    place(grid, 8, size - 1 - i, false);
  }
  for (let i = 0; i < 7; i += 1) {
    place(grid, size - 1 - i, 8, false);
  }

  // The one module that is dark in every code ever made. Written after the
  // format area, which stops one short of it.
  place(grid, size - 8, 8, true);

  if (version >= 7) {
    const bits = versionBits(version);
    for (let i = 0; i < 18; i += 1) {
      const dark = ((bits >> i) & 1) === 1;
      place(grid, Math.floor(i / 3), (i % 3) + size - 11, dark);
      place(grid, (i % 3) + size - 11, Math.floor(i / 3), dark);
    }
  }
}

function drawFormat(grid: Grid, mask: number): void {
  const size = grid.size;
  const bits = formatBits(mask);

  for (let i = 0; i < 15; i += 1) {
    const dark = ((bits >> i) & 1) === 1;

    // Down the left of the top-left finder, then on up the bottom-left one.
    if (i < 6) grid.modules[i][8] = dark;
    else if (i < 8) grid.modules[i + 1][8] = dark;
    else grid.modules[size - 15 + i][8] = dark;

    // And the same fifteen along the top, running out to the top-right finder.
    if (i < 8) grid.modules[8][size - 1 - i] = dark;
    else if (i === 8) grid.modules[8][15 - i] = dark;
    else grid.modules[8][14 - i] = dark;
  }
}

/**
 * Lays the codewords into the grid, two columns at a time from the right,
 * upward then downward, stepping over everything a function pattern claimed.
 * The mask is applied as each module is written.
 */
function drawData(grid: Grid, codewords: number[], mask: number): void {
  const size = grid.size;
  const masked = MASKS[mask];
  let bit = 0;
  let upward = true;

  for (let right = size - 1; right > 0; right -= 2) {
    // Column six is a timing pattern from top to bottom. The pairs step over it
    // rather than around it: from here on they are counted one column left.
    if (right === 6) {
      right = 5;
    }
    for (let step = 0; step < size; step += 1) {
      const row = upward ? size - 1 - step : step;
      for (const column of [right, right - 1]) {
        if (grid.fixed[row][column]) {
          continue;
        }
        // Past the end of the data are the remainder bits, which are zeros
        // before the mask and whatever the mask makes of them after it.
        const dark =
          bit < codewords.length * 8 &&
          ((codewords[bit >> 3] >> (7 - (bit & 7))) & 1) === 1;
        grid.modules[row][column] = dark !== masked(row, column);
        bit += 1;
      }
    }
    upward = !upward;
  }
}

/* ------------------------------------------------------------------ public */

/** How many bits the character count takes, which widens above version 9. */
function countBits(version: number): number {
  return version < 10 ? 8 : 16;
}

/** The data codewords a version holds, before correction codewords. */
function dataCodewords(version: number): number {
  return VERSIONS[version - 1].groups.reduce((total, [count, size]) => total + count * size, 0);
}

/** The smallest version that holds `bytes`, or nothing if none of them does. */
function versionFor(bytes: number): number | null {
  for (let version = 1; version <= VERSIONS.length; version += 1) {
    const room = dataCodewords(version) * 8 - 4 - countBits(version);
    if (bytes * 8 <= room) {
      return version;
    }
  }
  return null;
}

/** The data codewords: the header, the text, the terminator and the padding. */
function codewordsFor(bytes: Uint8Array, version: number): number[] {
  const capacity = dataCodewords(version);
  const codewords: number[] = [];
  let buffer = 0;
  let filled = 0;

  const push = (value: number, width: number) => {
    for (let i = width - 1; i >= 0; i -= 1) {
      buffer = (buffer << 1) | ((value >> i) & 1);
      filled += 1;
      if (filled === 8) {
        codewords.push(buffer);
        buffer = 0;
        filled = 0;
      }
    }
  };

  push(0b0100, 4);
  push(bytes.length, countBits(version));
  for (const byte of bytes) {
    push(byte, 8);
  }

  // The terminator is up to four zero bits, and then whatever it takes to end
  // on a codeword boundary.
  push(0, Math.min(4, (capacity - codewords.length) * 8 - filled));
  if (filled > 0) {
    push(0, 8 - filled);
  }

  // The two pad codewords the standard names, alternating to the end.
  for (let pad = 0xec; codewords.length < capacity; pad = pad === 0xec ? 0x11 : 0xec) {
    codewords.push(pad);
  }

  return codewords;
}

/** The blocks, each with its correction codewords, interleaved as they are drawn. */
function interleave(codewords: number[], version: number): number[] {
  const { ec, groups } = VERSIONS[version - 1];
  const data: number[][] = [];
  let taken = 0;

  for (const [count, size] of groups) {
    for (let i = 0; i < count; i += 1) {
      data.push(codewords.slice(taken, taken + size));
      taken += size;
    }
  }

  const correction = data.map((block) => remainder(block, ec));
  const widest = Math.max(...data.map((block) => block.length));
  const stream: number[] = [];

  for (let i = 0; i < widest; i += 1) {
    for (const block of data) {
      if (i < block.length) {
        stream.push(block[i]);
      }
    }
  }
  for (let i = 0; i < ec; i += 1) {
    for (const block of correction) {
      stream.push(block[i]);
    }
  }

  return stream;
}

/**
 * The code for `text`, at correction level M, in the smallest version that
 * holds it and under whichever of the eight masks scores best.
 *
 * Throws `QrTooLong` for text past version 20 — see the note at the top.
 */
export function encodeQr(text: string): QrMatrix {
  const bytes = new TextEncoder().encode(text);
  const version = versionFor(bytes.length);
  if (version === null) {
    throw new QrTooLong(`${bytes.length} bytes is more than a version ${VERSIONS.length} code holds`);
  }

  const stream = interleave(codewordsFor(bytes, version), version);
  const size = version * 4 + 17;

  let best: Grid | null = null;
  let bestScore = Number.POSITIVE_INFINITY;

  for (let mask = 0; mask < MASKS.length; mask += 1) {
    const grid = blank(size);
    drawFunctionPatterns(grid, version);
    drawData(grid, stream, mask);
    drawFormat(grid, mask);

    const score = penalty(grid.modules);
    if (score < bestScore) {
      best = grid;
      bestScore = score;
    }
  }

  return { size, modules: best!.modules };
}
