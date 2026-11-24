import type { Transition } from "framer-motion";

/** Shared easing: fast start, soft landing. */
export const ease = [0.22, 1, 0.36, 1] as const;

export const quick: Transition = { duration: 0.18, ease };
export const snappy: Transition = { duration: 0.15, ease };
export const settle: Transition = { duration: 0.22, ease };
