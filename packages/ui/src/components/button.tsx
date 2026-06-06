"use client";

import type * as React from "react";
import { tv, type VariantProps } from "tailwind-variants";
import { cn } from "../lib/utils";
import { Spinner } from "./spinner";

const buttonVariants = tv({
  base: `
    inline-flex shrink-0 items-center justify-center gap-2
    whitespace-nowrap rounded-xl font-semibold text-sm
    outline-none transition-all cursor-pointer
    focus-visible:ring-[3px] focus-visible:ring-primary/40
    disabled:pointer-events-none disabled:opacity-50
    [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0
    active:[&>*]:translate-y-px
  `,
  variants: {
    variant: {
      default:
        "bg-primary text-primary-foreground shadow-sm hover:bg-primary/90 active:bg-primary/80",
      destructive:
        "bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90 focus-visible:ring-destructive/40 active:bg-destructive/80",
      outline:
        "border border-border bg-card text-foreground hover:bg-muted-100 active:bg-muted-200",
      secondary: "bg-muted-200 text-secondary-foreground hover:bg-muted-300",
      dashed:
        "border border-border border-dashed bg-transparent text-muted-500 hover:border-primary/50 hover:bg-primary/5 hover:text-primary",
      ghost: "text-foreground hover:bg-muted-200",
      link: "text-primary decoration-primary/40 underline-offset-4 hover:underline",
    },
    size: {
      default: "h-10 px-4",
      sm: "h-8 gap-1.5 px-3",
      md: "h-9 px-3",
      lg: "h-11 px-5 text-base",
      icon: "size-9",
      "icon-sm": "size-7",
    },
  },
  defaultVariants: {
    variant: "default",
    size: "default",
  },
});

export type ButtonProps = VariantProps<typeof buttonVariants> &
  React.ComponentPropsWithoutRef<"button"> & {
    loading?: boolean;
    ref?: React.Ref<HTMLButtonElement>;
  };

export function Button({
  className,
  variant,
  size,
  children,
  disabled,
  loading,
  type = "button",
  ref,
  ...props
}: ButtonProps) {
  return (
    <button
      aria-busy={loading || undefined}
      className={cn(buttonVariants({ variant, size }), className)}
      data-slot="button"
      disabled={disabled || loading}
      ref={ref}
      type={type}
      {...props}
    >
      {loading ? (
        <>
          <Spinner aria-hidden size="sm" />
          {children}
        </>
      ) : (
        children
      )}
    </button>
  );
}

export { buttonVariants };
