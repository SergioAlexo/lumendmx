import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** A state updater that keeps the previous value when a polled result is
 * structurally the same, so polling an unchanged engine doesn't re-render. */
export function unlessEqual<T>(next: T): (prev: T) => T {
  return (prev) => (prev === next || JSON.stringify(prev) === JSON.stringify(next) ? prev : next);
}
