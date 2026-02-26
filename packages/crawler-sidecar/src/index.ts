import { createServer } from "./server";

const port = Number.parseInt(process.env.CRAWLER_PORT ?? "3791", 10);
const app = createServer();

app.listen(port, "127.0.0.1", () => {
  // eslint-disable-next-line no-console
  console.log(`[crawler-sidecar] listening on http://127.0.0.1:${port}`);
});
