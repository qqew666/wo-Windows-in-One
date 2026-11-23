import { randomAnswer, renderCaptcha } from "./captcha.mjs";

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const day = 86400;
const moscowOffset = 10800;
const captchaSeconds = 180;
const aad = encoder.encode("twi-captcha-v1");
const asciiBase64 = bytes => btoa(String.fromCharCode(...bytes));
const bytesFromBase64 = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));
const base64Url = bytes => asciiBase64(bytes).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");

async function captchaKey(env) {
  return crypto.subtle.importKey("raw", bytesFromBase64(env.TWI_CAPTCHA_SECRET), "AES-GCM", false, ["encrypt", "decrypt"]);
}

async function challenge(env, now) {
  const answer = randomAnswer();
  const key = await captchaKey(env);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const plaintext = encoder.encode(JSON.stringify({ answer, exp: now + captchaSeconds }));
  const ciphertext = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: aad }, key, plaintext));
  const ticket = new Uint8Array(iv.length + ciphertext.length);
  ticket.set(iv);
  ticket.set(ciphertext, iv.length);
  const png = await renderCaptcha(answer);
  return { ticket: asciiBase64(ticket), image: "data:image/png;base64," + asciiBase64(png) };
}

async function verify(env, data, now) {
  const { ticket, answer } = data ?? {};
  if (typeof ticket !== "string" || ticket.length > 256 ||
      typeof answer !== "string" || !/^[2-9A-HJ-NP-Z]{6}$/i.test(answer.trim())) return false;

  const key = await captchaKey(env);
  try {
    const bytes = bytesFromBase64(ticket);
    const clear = await crypto.subtle.decrypt({
      name: "AES-GCM", iv: bytes.subarray(0, 12), additionalData: aad
    }, key, bytes.subarray(12));
    const expected = JSON.parse(decoder.decode(clear));
    return now < expected.exp && answer.trim().toUpperCase() === expected.answer;
  } catch {
    return false;
  }
}

async function dailyKey(env, now) {
  const pem = env.TWI_SIGNING_PRIVATE_PEM.replace(/-----[^-]+-----|\s/g, "");
  const key = await crypto.subtle.importKey("pkcs8", bytesFromBase64(pem),
    { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["sign"]);
  const starts = Math.floor((now + moscowOffset) / day) * day - moscowOffset;
  const ends = starts + day;
  const payload = `TWI1.${starts}.${ends}`;
  const signature = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, encoder.encode(payload));
  return { value: payload + "." + base64Url(new Uint8Array(signature)), ends };
}

async function readAnswer(request) {
  const reader = request.body?.getReader();
  if (!reader) return null;
  let total = 0;
  let text = "";
  const input = new TextDecoder();
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) return JSON.parse(text + input.decode());
      total += value.byteLength;
      if (total > 1024) { await reader.cancel(); return null; }
      text += input.decode(value, { stream: true });
    }
  } catch {
    return null;
  }
}

export function makeWorker(assets = {}) {
  return {
    async fetch(request, env) {
      const url = new URL(request.url);
      if (!url.pathname.startsWith("/api/")) {
        const asset = assets[url.pathname];
        if (!asset || !["GET", "HEAD"].includes(request.method)) return new Response("Not found", { status: 404 });
        return new Response(request.method === "HEAD" ? null : bytesFromBase64(asset.data), {
          headers: { "Content-Type": asset.type, "X-Content-Type-Options": "nosniff" }
        });
      }

      const origin = request.headers.get("Origin");
      if (origin && origin !== url.origin && origin !== env.TWI_SITE_ORIGIN) {
        return new Response(null, { status: 403, headers: { Vary: "Origin" } });
      }

      const headers = new Headers({
        "Cache-Control": "no-store",
        "Content-Type": "application/json; charset=utf-8",
        "X-Content-Type-Options": "nosniff",
        Vary: "Origin"
      });
      if (origin) headers.set("Access-Control-Allow-Origin", origin);
      function json(data, status = 200) {
        return new Response(JSON.stringify(data), { status, headers });
      }

      if (request.method === "OPTIONS") {
        headers.set("Access-Control-Allow-Methods", "POST, OPTIONS");
        headers.set("Access-Control-Allow-Headers", "Content-Type");
        return new Response(null, { status: 204, headers });
      }
      if (request.method !== "POST") return json({ error: "method" }, 405);
      if (!env.TWI_CAPTCHA_SECRET || !env.TWI_SIGNING_PRIVATE_PEM) return json({ error: "unavailable" }, 503);

      const now = Math.floor(Date.now() / 1000);
      try {
        if (url.pathname === "/api/challenge") return json(await challenge(env, now));
        if (url.pathname !== "/api/key") return json({ error: "not_found" }, 404);
        const data = await readAnswer(request);
        if (!(await verify(env, data, now))) return json({ error: "captcha" }, 422);
        return json(await dailyKey(env, now));
      } catch {
        return json({ error: "unavailable" }, 503);
      }
    }
  };
}

export default makeWorker();
