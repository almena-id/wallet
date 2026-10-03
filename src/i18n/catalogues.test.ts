import { describe, expect, it } from "vitest";

import en from "./messages/en.json";
import es from "./messages/es.json";
import { fill, plural } from "./format";

type Tree = { [key: string]: string | Tree };

/** Every string of a catalogue, by its dotted key. */
function leaves(tree: Tree, prefix = ""): Map<string, string> {
  const found = new Map<string, string>();
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === "string") {
      found.set(path, value);
    } else {
      for (const [inner, text] of leaves(value, path)) {
        found.set(inner, text);
      }
    }
  }
  return found;
}

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("catalogues", () => {
  const english = leaves(en as Tree);
  const spanish = leaves(es as Tree);

  it("translate every key, and only those", () => {
    expect([...spanish.keys()].sort()).toEqual([...english.keys()].sort());
  });

  it("keep every placeholder of the English", () => {
    for (const [key, text] of english) {
      expect(placeholders(spanish.get(key) ?? ""), key).toEqual(placeholders(text));
    }
  });
});

describe("format", () => {
  it("fills placeholders and leaves unknown ones", () => {
    expect(fill("{a} of {b}", { a: 2, b: 4 })).toBe("2 of 4");
    expect(fill("{a} {missing}", { a: "x" })).toBe("x {missing}");
  });

  it("picks the plural form the locale calls for, numbers written its way", () => {
    const forms = { one: "{count} item", other: "{count} items" };
    expect(plural(forms, 1, "en")).toBe("1 item");
    expect(plural(forms, 1200, "en")).toBe("1,200 items");
    expect(plural(forms, 1200, "es")).toBe("1200 items");
  });
});
