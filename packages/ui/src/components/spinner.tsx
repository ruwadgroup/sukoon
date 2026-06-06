import type * as React from "react";
import { tv, type VariantProps } from "tailwind-variants";
import { cn } from "../lib/utils";

export const spinnerVariants = tv({
  base: "animate-spin",
  variants: {
    size: {
      sm: "size-4",
      md: "size-5",
      lg: "size-6",
      xl: "size-8",
    },
    variant: {
      default: "text-current",
      primary: "text-primary",
      muted: "text-muted-500",
      white: "text-white",
    },
  },
  defaultVariants: {
    size: "md",
    variant: "default",
  },
});

export type SpinnerProps = React.ComponentProps<"svg"> &
  VariantProps<typeof spinnerVariants> & {
    label?: string;
  };

export function Spinner({ className, size, variant, label = "Loading", ...props }: SpinnerProps) {
  return (
    <svg
      aria-label={label}
      className={cn(spinnerVariants({ size, variant }), className)}
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <path d="M21 12a9 9 0 1 1-6.219-8.56" />
    </svg>
  );
}
