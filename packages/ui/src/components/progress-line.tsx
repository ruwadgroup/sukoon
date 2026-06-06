export function ProgressLine({
  value,
  indeterminate = false,
  className,
}: {
  value?: number;
  indeterminate?: boolean;
  className?: string;
}) {
  const safe = Math.max(0, Math.min(value ?? 0, 100));

  return (
    <div className={`h-1.5 w-full overflow-hidden rounded-full bg-muted-200 ${className ?? ""}`}>
      <div
        className="h-full rounded-full bg-primary transition-[width] duration-200"
        style={{ width: indeterminate ? "30%" : `${Math.max(3, safe)}%` }}
      />
    </div>
  );
}
