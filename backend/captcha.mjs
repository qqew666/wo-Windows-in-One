const glyphs = {
  2: "0E11110204081F", 3: "1E01010E01011E", 4: "02060A121F0202", 5: "1F10101E01011E",
  6: "0E10101E11110E", 7: "1F010204080808", 8: "0E11110E11110E", 9: "0E11110F01010E",
  A: "0E11111F111111", B: "1E11111E11111E", C: "0E11101010110E", D: "1E11111111111E",
  E: "1F10101E10101F", F: "1F10101E101010", G: "0E11101711110F", H: "1111111F111111",
  J: "0702020212120C", K: "11121418141211", L: "1010101010101F", M: "111B1515111111",
  N: "11191915131311", P: "1E11111E101010", Q: "0E11111115120D", R: "1E11111E141211",
  S: "0F10100E01011E", T: "1F040404040404", U: "1111111111110E", V: "111111110A0A04",
  W: "11111115151B11", X: "11110A040A1111", Y: "11110A04040404", Z: "1F01020408101F"
};
const alphabet = Object.keys(glyphs);
const width = 260;
const height = 84;
const header = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
const encoder = new TextEncoder();

export function randomAnswer() {
  return Array.from(crypto.getRandomValues(new Uint8Array(6)), byte => alphabet[byte & 31]).join("");
}

function color(hex) {
  return [hex >> 16, (hex >> 8) & 255, hex & 255];
}

function pngChunk(name, data) {
  const result = new Uint8Array(data.length + 12);
  const view = new DataView(result.buffer);
  view.setUint32(0, data.length);
  result.set(encoder.encode(name), 4);
  result.set(data, 8);
  let crc = 0xffffffff;
  for (const value of result.subarray(4, -4)) {
    crc ^= value;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  view.setUint32(result.length - 4, crc ^ 0xffffffff);
  return result;
}

export async function renderCaptcha(answer) {
  const stride = width * 3 + 1;
  const pixels = new Uint8Array(stride * height);
  const random = crypto.getRandomValues(new Uint16Array(2300));
  let cursor = 0;
  const next = max => random[cursor++] % max;
  const background = color(0xe5f0d7);
  const noise = [0xbed1b5, 0x72917d, 0x569278, 0xa1ad71].map(color);

  function dot(x, y, shade) {
    x = Math.round(x);
    y = Math.round(y);
    if (x >= 0 && x < width && y >= 0 && y < height) pixels.set(shade, y * stride + x * 3 + 1);
  }

  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) dot(x, y, background);
  }

  for (let point = 0; point < 515; point++) dot(next(width), next(height), noise[next(noise.length)]);

  function wave(shade) {
    const baseline = next(height);
    const phase = next(40);
    const amplitude = 10 + next(19);
    const period = 16 + next(14);
    for (let x = 0; x < width; x++) dot(x, baseline + Math.sin((x + phase) / period) * amplitude, shade);
  }

  for (const shade of noise) wave(shade);
  Array.from(answer).forEach((symbol, index) => {
    const baseX = 18 + index * 39 + next(9);
    const baseY = 13 + next(13);
    const angle = (next(40) - 20) * Math.PI / 180;
    const cosine = Math.cos(angle);
    const sine = Math.sin(angle);
    const shade = color([0x203e30, 0x233d56, 0x344728, 0x563251][next(4)]);
    const rows = glyphs[symbol].match(/../g).map(row => Number.parseInt(row, 16));
    for (let row = 0; row < rows.length; row++) {
      for (let column = 0; column < 5; column++) {
        if (!(rows[row] & (16 >> column))) continue;
        for (let dy = 0; dy < 6; dy++) {
          for (let dx = 0; dx < 6; dx++) {
            const x = column * 5 + dx - 12;
            const y = row * 6 + dy - 20;
            const bend = Math.sin((y + index * 6) / 9) * 3;
            dot(baseX + 12 + cosine * x - sine * y + bend, baseY + 20 + sine * x + cosine * y, shade);
          }
        }
      }
    }
  });
  wave(noise[0]);
  wave(noise[2]);

  const dimensions = new Uint8Array(13);
  const view = new DataView(dimensions.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  dimensions.set([8, 2, 0, 0, 0], 8);
  const compressed = new Blob([pixels]).stream().pipeThrough(new CompressionStream("deflate"));
  const data = new Uint8Array(await new Response(compressed).arrayBuffer());
  const png = [header, pngChunk("IHDR", dimensions), pngChunk("IDAT", data), pngChunk("IEND", new Uint8Array())];
  const result = new Uint8Array(png.reduce((length, bytes) => length + bytes.length, 0));
  let offset = 0;
  for (const bytes of png) {
    result.set(bytes, offset);
    offset += bytes.length;
  }
  return result;
}
