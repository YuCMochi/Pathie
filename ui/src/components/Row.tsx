import { cn } from '../lib/utils';

export function Row({
  title,
  desc,
  disabled,
  children,
}: {
  title: string;
  desc?: string;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-1 flex min-h-14 items-center justify-between gap-4 rounded-lg border border-border bg-card px-4 py-3">
      <div className={cn(disabled && 'opacity-50')}>
        <div>{title}</div>
        {desc && <div className="mt-0.5 text-xs leading-normal text-muted-foreground">{desc}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
