export function formatUptime(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86400) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return m > 0 ? `${h}h ${m}m` : `${h}h`;
  }
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  return h > 0 ? `${d}d ${h}h` : `${d}d`;
}

export function formatEntrypoint(entrypoint: string): string {
  if (entrypoint.includes("vscode")) return "VS Code";
  if (entrypoint.includes("cursor")) return "Cursor";
  if (entrypoint.includes("cli")) return "CLI";
  if (entrypoint === "") return "CLI";
  return entrypoint;
}

export function formatTokens(tokens: number): string {
  if (tokens < 1000) return `${tokens}`;
  if (tokens < 1_000_000) return `${(tokens / 1000).toFixed(1)}k`;
  return `${(tokens / 1_000_000).toFixed(2)}M`;
}

export function formatModel(model: string | null): string {
  if (!model) return "Unknown";
  if (model.includes("opus")) return "Opus 4";
  if (model.includes("sonnet")) return "Sonnet 4";
  if (model.includes("haiku")) return "Haiku 3.5";
  return model;
}
