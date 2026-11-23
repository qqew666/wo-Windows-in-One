import { createPrivateKey, createPublicKey, sign } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";

const day = 86400;
const moscowOffset = 10800;
const pem = process.env.TWI_SIGNING_PRIVATE_PEM;
if (!pem) throw new Error("Set TWI_SIGNING_PRIVATE_PEM to the RSA private PEM");

const privateKey = createPrivateKey(pem);
if (privateKey.asymmetricKeyType !== "rsa" || privateKey.asymmetricKeyDetails.modulusLength !== 2048) {
  throw new Error("An RSA 2048-bit signing key is required");
}

const publicKey = createPublicKey(privateKey).export({ format: "pem", type: "spki" });
const configuredKey = readFileSync(new URL("../app/license-public.pem", import.meta.url), "utf8");
if (publicKey.trim() !== configuredKey.trim()) throw new Error("Signing key does not match app/license-public.pem");

const now = Math.floor(Date.now() / 1000);
const dayStart = Math.floor((now + moscowOffset) / day) * day - moscowOffset;
const keys = [dayStart, dayStart + day].map(starts => {
  const ends = starts + day;
  const payload = `TWI1.${starts}.${ends}`;
  const signature = sign("sha256", Buffer.from(payload, "ascii"), privateKey).toString("base64url");
  return { starts, ends, value: `${payload}.${signature}` };
});

const output = process.argv[2] ?? new URL("../site/keys.json", import.meta.url);
writeFileSync(output, JSON.stringify({ keys }) + "\n");
