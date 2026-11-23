import * as React from "react";

import { cn } from "@/lib/utils";

export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("rounded-3xl border border-white/10 bg-[#1c1814]/80 p-5 shadow-[0_20px_60px_rgba(0,0,0,0.25)]", className)}
      {...props}
    />
  );
}
