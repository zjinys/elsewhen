import { describe, expect, it } from "vitest";
import { diffItems, itemHash, type Item } from "../src/diff";

const repo = (key: string, stars: number): Item => ({ id: key, name: key, stars });

describe("itemHash", () => {
  it("is order-insensitive", () => {
    expect(itemHash({ id: "x", stars: 5, name: "x" })).toBe(
      itemHash({ name: "x", id: "x", stars: 5 })
    );
  });
  it("changes with content", () => {
    expect(itemHash({ id: "x", stars: 5 })).not.toBe(itemHash({ id: "x", stars: 6 }));
  });
});

describe("diffItems", () => {
  it("first run: all new", () => {
    const f = diffItems([repo("a", 1), repo("b", 2)], null);
    expect(f.map((x) => [x.kind, x.item_key]).sort()).toEqual([
      ["new", "a"],
      ["new", "b"],
    ]);
  });

  it("empty previous: all new", () => {
    expect(diffItems([repo("a", 1)], [])).toHaveLength(1);
  });

  it("no change: no findings", () => {
    expect(diffItems([repo("a", 5), repo("b", 3)], [repo("a", 5), repo("b", 3)])).toEqual([]);
  });

  it("changed detected", () => {
    const f = diffItems([repo("a", 9), repo("b", 3)], [repo("a", 5), repo("b", 3)]);
    expect(f).toHaveLength(1);
    expect(f[0].kind).toBe("changed");
    expect((f[0].payload as Item).stars).toBe(9);
  });

  it("gone detected", () => {
    const f = diffItems([repo("a", 5)], [repo("a", 5), repo("b", 3)]);
    // gone payload 携带最后一次见到的完整 item（消费端展示「什么消失了」）。
    expect(f).toEqual([
      { kind: "gone", item_key: "b", payload: { id: "b", name: "b", stars: 3 } },
    ]);
  });

  it("mixed kinds", () => {
    const f = diffItems(
      [repo("a", 5), repo("c", 1)],
      [repo("a", 4), repo("b", 2)]
    );
    const got = new Set(f.map((x) => [x.item_key, x.kind] as const));
    expect(got).toEqual(
      new Set([
        ["a", "changed"],
        ["b", "gone"],
        ["c", "new"],
      ])
    );
  });

  it("missing id raises", () => {
    expect(() =>
      diffItems([{ name: "no-id" } as unknown as Item], null)
    ).toThrow(/missing stable 'id'/);
  });
});

describe("delta", () => {
  it("changed carries field-level from/to/diff", () => {
    const f = diffItems([repo("a", 9)], [repo("a", 5)]);
    expect(f).toHaveLength(1);
    expect(f[0].delta).toEqual({ stars: { from: 5, to: 9, diff: 4 } });
  });

  it("delta skips id and unchanged fields", () => {
    const f = diffItems(
      [{ id: "a", name: "same", stars: 9 }],
      [{ id: "a", name: "same", stars: 5 }]
    );
    expect(Object.keys(f[0].delta!)).toEqual(["stars"]);
  });

  it("non-numeric change has no diff", () => {
    const f = diffItems(
      [{ id: "a", description: "new" }],
      [{ id: "a", description: "old" }]
    );
    expect(f[0].delta).toEqual({ description: { from: "old", to: "new" } });
  });

  it("new and gone carry no delta", () => {
    const f = diffItems([repo("b", 1)], [repo("a", 2)]);
    for (const x of f) expect(x.delta).toBeUndefined();
  });

  it("delta detail is bounded", () => {
    const prev: Item = { id: "a" };
    const curr: Item = { id: "a" };
    for (let i = 0; i < 40; i++) {
      prev["f" + i] = i;
      curr["f" + i] = i + 1;
    }
    const f = diffItems([curr], [prev]);
    expect(Object.keys(f[0].delta!).length).toBe(12);
  });
});
