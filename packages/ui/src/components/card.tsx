"use client";

import type { ReactNode } from "react";
import { tv, type VariantProps } from "tailwind-variants";
import { cn } from "../lib/utils";

const cardVariants = tv({
  slots: {
    base: "relative overflow-hidden rounded-2xl",
    header: "flex flex-col gap-1 px-6 pt-6 pb-4",
    title: "font-semibold text-base text-card-foreground",
    description: "text-muted-500 text-sm leading-5",
    content: "px-6 py-6",
    footer: "flex items-center gap-2 px-6 pt-4 pb-6",
  },
  variants: {
    variant: {
      default: { base: "glass" },
      flat: { base: "field border rounded-xl" },
      subdued: { base: "bg-muted-100 rounded-xl" },
    },
  },
  defaultVariants: { variant: "default" },
});

export type CardProps = Omit<React.ComponentProps<"div">, "title"> &
  VariantProps<typeof cardVariants> & {
    title?: ReactNode;
    description?: ReactNode;
    footer?: ReactNode;
    classNames?: { header?: string; content?: string; footer?: string };
  };

export function Card({
  className,
  variant,
  title,
  description,
  footer,
  classNames,
  children,
  ...props
}: CardProps) {
  const slots = cardVariants({ variant });
  const hasHeader = title || description;

  return (
    <div className={cn(slots.base(), className)} data-slot="card" {...props}>
      {hasHeader && (
        <div className={cn(slots.header(), classNames?.header)} data-slot="card-header">
          {title && <h3 className={slots.title()}>{title}</h3>}
          {description && <div className={slots.description()}>{description}</div>}
        </div>
      )}
      {children && (
        <div className={cn(slots.content(), classNames?.content)} data-slot="card-content">
          {children}
        </div>
      )}
      {footer && (
        <div className={cn(slots.footer(), classNames?.footer)} data-slot="card-footer">
          {footer}
        </div>
      )}
    </div>
  );
}
