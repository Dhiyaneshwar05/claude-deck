interface StatusDotProps {
  alive: boolean;
  size?: "sm" | "md";
}

export function StatusDot({ alive, size = "sm" }: StatusDotProps) {
  const px = size === "sm" ? "h-2 w-2" : "h-2.5 w-2.5";
  return (
    <span
      className={`inline-block rounded-full ${px} ${
        alive
          ? "bg-emerald-400 shadow-[0_0_6px_rgba(52,211,153,0.5)]"
          : "bg-zinc-600"
      }`}
    />
  );
}
