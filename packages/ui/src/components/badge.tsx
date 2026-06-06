"use client";

import type * as React from "react";
import { tv, type VariantProps } from "tailwind-variants";
import { cn } from "../lib/utils";

export const badgeVariants = tv({
  base: "inline-flex w-fit shrink-0 items-center gap-1 whitespace-nowrap font-medium",
  variants: {
    variant: {
      default: "bg-secondary text-secondary-foreground",
      info: "bg-info/10 text-info-soft-foreground",
      success: "bg-success/10 text-success-soft-foreground",
      warning: "bg-warning/10 text-warning-soft-foreground",
      destructive: "bg-destructive/10 text-destructive-soft-foreground",
      "success-strong": "bg-success text-success-foreground",
      "destructive-strong": "bg-destructive text-destructive-foreground",
    },
    size: {
      sm: "rounded px-1.5 py-0.5 text-xs [&_svg]:size-3",
      md: "rounded-md px-2 py-0.5 text-xs [&_svg]:size-3",
      lg: "rounded-lg px-2.5 py-1 text-sm [&_svg]:size-4",
    },
  },
  defaultVariants: { variant: "default", size: "md" },
});

export type BadgeProps = React.ComponentProps<"span"> & VariantProps<typeof badgeVariants>;

export function Badge({ className, variant, size, children, ...props }: BadgeProps) {
  return (
    <span className={cn(badgeVariants({ variant, size }), className)} data-slot="badge" {...props}>
      {children}
    </span>
  );
}
