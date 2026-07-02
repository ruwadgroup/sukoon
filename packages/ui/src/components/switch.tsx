"use client";

import type * as React from "react";
import { tv, type VariantProps } from "tailwind-variants";
import { cn } from "../lib/utils";

export const switchVariants = tv({
  base:
    "relative inline-flex shrink-0 cursor-pointer items-center rounded-full transition-colors " +
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 " +
    "disabled:cursor-not-allowed disabled:opacity-50",
  variants: {
    size: {
      sm: "h-5 w-9",
      md: "h-6 w-11",
    },
    checked: {
      true: "bg-primary",
      false: "bg-muted-500/25",
    },
  },
  defaultVariants: { size: "md", checked: false },
});

const thumbVariants = tv({
  base: "pointer-events-none block rounded-full bg-card shadow-sm transition-transform",
  variants: {
    size: {
      sm: "size-4",
      md: "size-5",
    },
    checked: {
      true: "",
      false: "translate-x-0.5",
    },
  },
  compoundVariants: [
    { size: "sm", checked: true, class: "translate-x-[18px]" },
    { size: "md", checked: true, class: "translate-x-[22px]" },
  ],
  defaultVariants: { size: "md", checked: false },
});

export type SwitchProps = Omit<React.ComponentProps<"button">, "onChange"> &
  VariantProps<typeof switchVariants> & {
    checked: boolean;
    onCheckedChange?: (checked: boolean) => void;
  };

export function Switch({ className, size, checked, onCheckedChange, ...props }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      data-slot="switch"
      onClick={() => onCheckedChange?.(!checked)}
      className={cn(switchVariants({ size, checked }), className)}
      {...props}
    >
      <span className={thumbVariants({ size, checked })} />
    </button>
  );
}
