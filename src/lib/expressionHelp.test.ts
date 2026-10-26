import { describe, expect, it } from "vitest";
import engine from "../../src-tauri/src/graph/expr.rs?raw";
import { CONSTANTS, FUNCTIONS } from "./expressionHelp";

// The engine's list is read from its source, so the help can't drift from it.

describe("expression help", () => {
  it("lists exactly the functions the engine knows, with their argument counts", () => {
    const known = [...engine.matchAll(/"(\w+)" => \(Func::\w+, (\d)\)/g)].map((m) => `${m[1]}/${m[2]}`).sort();
    const listed = FUNCTIONS.map((f) => `${f.signature.split("(")[0]}/${f.signature.split(",").length}`).sort();
    expect(listed).toEqual(known);
  });

  it("lists exactly the constants the engine knows", () => {
    const known = [...engine.matchAll(/"(\w+)" => Some\(/g)].map((m) => m[1]).sort();
    expect(CONSTANTS.map((c) => c.name).sort()).toEqual(known);
  });
});
