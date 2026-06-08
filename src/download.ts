async function fetchOk(url: string): Promise<Response> {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`GET ${url} -> ${res.status} ${res.statusText}`);
  return res;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}KB`;
  return `${(n / 1024 / 1024).toFixed(2)}MB`;
}

function renderBar(downloaded: number, total: number): string {
  const BAR = 30;
  if (total <= 0) return `    ${formatBytes(downloaded)} downloaded`;
  const ratio = Math.min(1, downloaded / total);
  const filled = Math.floor(ratio * BAR);
  const head = filled < BAR ? ">" : "";
  const bar = "=".repeat(filled) + head + " ".repeat(Math.max(0, BAR - filled - 1));
  const pct = (ratio * 100).toFixed(1).padStart(5);
  return `    [${bar}] ${pct}% ${formatBytes(downloaded)}/${formatBytes(total)}`;
}

export async function downloadWithProgress(url: string): Promise<Uint8Array> {
  const res = await fetchOk(url);
  const total = Number(res.headers.get("content-length") ?? 0);
  if (!res.body) return new Uint8Array(await res.arrayBuffer());

  const isTty = Boolean(process.stdout.isTTY);
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let downloaded = 0;
  let lastRender = 0;

  const render = (force: boolean): void => {
    if (!isTty) return;
    const now = Date.now();
    if (!force && now - lastRender < 100) return;
    lastRender = now;
    process.stdout.write(`\r${renderBar(downloaded, total)}`);
  };

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    downloaded += value.length;
    render(false);
  }
  render(true);
  if (isTty) process.stdout.write("\n");
  else console.log(renderBar(downloaded, total).trimStart());

  const out = new Uint8Array(downloaded);
  let off = 0;
  for (const c of chunks) {
    out.set(c, off);
    off += c.length;
  }
  return out;
}
