// What an Expression node's formula can use, for the help on the node.
// Keep in step with `Func::named` and `constant` in src-tauri/src/graph/expr.rs
// (expressionHelp.test.ts checks the names).

export const OPERATORS: { symbol: string; meaning: string }[] = [
  { symbol: "+  -  *  /", meaning: "add, subtract, multiply, divide (x / 0 is 0)" },
  { symbol: "%", meaning: "remainder: 7 % 3 is 1" },
  { symbol: "^", meaning: "power: 2 ^ 3 is 8" },
  { symbol: "( )", meaning: "brackets" },
];

export const CONSTANTS: { name: string; meaning: string }[] = [
  { name: "pi", meaning: "3.14159" },
  { name: "tau", meaning: "6.28318, a full turn" },
  { name: "e", meaning: "2.71828" },
];

export const FUNCTIONS: { signature: string; meaning: string }[] = [
  { signature: "sin(x)", meaning: "sine; x in radians" },
  { signature: "cos(x)", meaning: "cosine; x in radians" },
  { signature: "abs(x)", meaning: "without the minus sign" },
  { signature: "sqrt(x)", meaning: "square root (0 for negatives)" },
  { signature: "floor(x)", meaning: "round down: 2.7 is 2" },
  { signature: "fract(x)", meaning: "the part after the point: 2.75 is 0.75" },
  { signature: "sign(x)", meaning: "-1, 0 or 1" },
  { signature: "min(a, b)", meaning: "the smaller" },
  { signature: "max(a, b)", meaning: "the larger" },
  { signature: "pow(a, b)", meaning: "a to the power b, like a ^ b" },
  { signature: "step(edge, x)", meaning: "0 below edge, 1 from it" },
  { signature: "clamp(x, lo, hi)", meaning: "keep x between lo and hi" },
  { signature: "mix(a, b, t)", meaning: "a at t = 0, b at t = 1" },
  { signature: "smoothstep(lo, hi, x)", meaning: "eases from 0 at lo to 1 at hi" },
];
