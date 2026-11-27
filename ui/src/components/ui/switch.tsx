import * as SwitchPrimitive from '@radix-ui/react-switch';
import { cn } from '../../lib/utils';

export function Switch({ className, ...props }: React.ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      className={cn(
        'inline-flex h-5 w-10 shrink-0 items-center rounded-full border border-input transition-colors',
        'data-[state=checked]:border-primary data-[state=checked]:bg-primary disabled:cursor-default disabled:opacity-40',
        'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary',
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb className="block size-3 translate-x-[3px] rounded-full bg-input transition-transform data-[state=checked]:translate-x-[23px] data-[state=checked]:bg-primary-foreground" />
    </SwitchPrimitive.Root>
  );
}
