import { createServer } from "node:http";
import { Readable } from "node:stream";
import worker from "../dist/server/index.js";

const port = Number(process.env.TWI_PORT ?? 8080);
const host = process.env.TWI_HOST ?? "127.0.0.1";
const origin = new URL(process.env.TWI_PUBLIC_ORIGIN ?? `http://127.0.0.1:${port}`).origin;

const server = createServer({ requestTimeout: 15_000 }, async (incoming, outgoing) => {
  try {
    const url = new URL(incoming.url, origin);
    if (url.origin !== origin) {
      outgoing.writeHead(400).end();
      return;
    }

    const method = incoming.method;
    const request = new Request(url, {
      method,
      headers: incoming.headers,
      ...(method === "GET" || method === "HEAD"
        ? {}
        : { body: Readable.toWeb(incoming), duplex: "half" })
    });
    const response = await worker.fetch(request, process.env);
    const body = response.body ? Buffer.from(await response.arrayBuffer()) : undefined;
    outgoing.writeHead(response.status, Object.fromEntries(response.headers)).end(body);
  } catch {
    if (!outgoing.headersSent) {
      outgoing.writeHead(500, { "Cache-Control": "no-store" }).end();
    } else {
      outgoing.destroy();
    }
  }
});

server.listen(port, host, () => {
  console.log(`Two Windows in One listening on ${host}:${port}`);
});
